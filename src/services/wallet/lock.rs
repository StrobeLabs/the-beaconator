//! Distributed wallet locking using Redis
//!
//! Provides distributed locks to ensure only one beaconator instance
//! can use a wallet at a time, preventing nonce conflicts.

use alloy::primitives::Address;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::models::wallet::PrefixedRedisKeys;

/// Lua script: extend the lock TTL only if we still hold it.
const EXTEND_SCRIPT: &str = r#"
    if redis.call("get", KEYS[1]) == ARGV[1] then
        return redis.call("pexpire", KEYS[1], ARGV[2])
    else
        return 0
    end
"#;

/// Lua script: delete the lock only if we still hold it.
const RELEASE_SCRIPT: &str = r#"
    if redis.call("get", KEYS[1]) == ARGV[1] then
        return redis.call("del", KEYS[1])
    else
        return 0
    end
"#;

/// A distributed lock for a specific wallet
pub struct WalletLock {
    conn: ConnectionManager,
    wallet_address: Address,
    instance_id: String,
    lock_key: String,
    ttl: Duration,
}

impl WalletLock {
    /// Create a new wallet lock with default "beaconator:" prefix
    pub fn new(
        conn: ConnectionManager,
        wallet_address: Address,
        instance_id: String,
        ttl: Duration,
    ) -> Self {
        Self::with_keys(
            conn,
            wallet_address,
            instance_id,
            ttl,
            &PrefixedRedisKeys::default(),
        )
    }

    /// Create a new wallet lock with a custom key generator
    ///
    /// This allows using a custom prefix for test isolation.
    pub fn with_keys(
        conn: ConnectionManager,
        wallet_address: Address,
        instance_id: String,
        ttl: Duration,
        keys: &PrefixedRedisKeys,
    ) -> Self {
        let lock_key = keys.wallet_lock(&wallet_address);
        Self {
            conn,
            wallet_address,
            instance_id,
            lock_key,
            ttl,
        }
    }

    /// Create a lock that serializes ECDSA updates for one BEACON (the locked
    /// address is the beacon, not a pool wallet). Same acquire/heartbeat/release
    /// semantics as a wallet lock, distinct Redis key namespace.
    pub fn for_beacon_update(
        conn: ConnectionManager,
        beacon_address: Address,
        instance_id: String,
        ttl: Duration,
        keys: &PrefixedRedisKeys,
    ) -> Self {
        let lock_key = keys.beacon_update_lock(&beacon_address);
        Self {
            conn,
            wallet_address: beacon_address,
            instance_id,
            lock_key,
            ttl,
        }
    }

    /// Get a Redis connection (cheap clone of the shared auto-reconnecting manager)
    fn get_conn(&self) -> ConnectionManager {
        self.conn.clone()
    }

    /// Attempt to acquire the lock with retries
    ///
    /// # Arguments
    /// * `max_retries` - Maximum number of attempts (must be >= 1, 0 is treated as 1)
    /// * `retry_delay` - Duration to wait between attempts
    pub async fn acquire(
        &self,
        max_retries: u32,
        retry_delay: Duration,
    ) -> Result<WalletLockGuard, String> {
        // Ensure at least one attempt
        let attempts = max_retries.max(1);

        for attempt in 0..attempts {
            match self.try_acquire().await {
                Ok(guard) => {
                    tracing::debug!(
                        "Acquired lock for wallet {} on attempt {}",
                        self.wallet_address,
                        attempt + 1
                    );
                    return Ok(guard);
                }
                Err(e) if attempt < attempts - 1 => {
                    tracing::debug!(
                        "Lock acquisition attempt {} failed for wallet {}: {}",
                        attempt + 1,
                        self.wallet_address,
                        e
                    );
                    tokio::time::sleep(retry_delay).await;
                }
                Err(e) => {
                    return Err(format!(
                        "Failed to acquire lock for wallet {} after {} attempts: {}",
                        self.wallet_address, attempts, e
                    ));
                }
            }
        }

        // This should be unreachable since we always have at least 1 attempt,
        // but we return an error instead of panicking for safety
        Err(format!(
            "Failed to acquire lock for wallet {}: no attempts made",
            self.wallet_address
        ))
    }

    /// Try to acquire the lock once (non-blocking)
    pub async fn try_acquire(&self) -> Result<WalletLockGuard, String> {
        let mut conn = self.get_conn();

        // SET NX with TTL (atomic operation)
        // SET key value NX PX milliseconds
        let result: Option<String> = redis::cmd("SET")
            .arg(&self.lock_key)
            .arg(&self.instance_id)
            .arg("NX")
            .arg("PX")
            .arg(self.ttl.as_millis() as u64)
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to acquire lock: {e}"))?;

        match result {
            Some(_) => {
                tracing::info!(
                    "Acquired distributed lock for wallet {} (instance: {})",
                    self.wallet_address,
                    self.instance_id
                );
                Ok(WalletLockGuard {
                    conn: self.conn.clone(),
                    lock_key: self.lock_key.clone(),
                    instance_id: self.instance_id.clone(),
                    wallet_address: self.wallet_address,
                })
            }
            None => {
                // Lock is held by another instance - try to get holder info
                let holder: Option<String> = conn.get(&self.lock_key).await.ok().flatten();

                Err(format!(
                    "Lock for wallet {} is held by instance: {}",
                    self.wallet_address,
                    holder.unwrap_or_else(|| "unknown".to_string())
                ))
            }
        }
    }

    /// Check if the lock is currently held (by anyone)
    pub async fn is_locked(&self) -> Result<bool, String> {
        let mut conn = self.get_conn();

        let exists: bool = conn
            .exists(&self.lock_key)
            .await
            .map_err(|e| format!("Failed to check lock status: {e}"))?;

        Ok(exists)
    }

    /// Get the current lock holder (if any)
    pub async fn get_holder(&self) -> Result<Option<String>, String> {
        let mut conn = self.get_conn();

        let holder: Option<String> = conn
            .get(&self.lock_key)
            .await
            .map_err(|e| format!("Failed to get lock holder: {e}"))?;

        Ok(holder)
    }

    /// Extend the lock TTL (only if we hold the lock)
    pub async fn extend(&self, new_ttl: Duration) -> Result<bool, String> {
        let mut conn = self.get_conn();

        let extended: i32 = redis::Script::new(EXTEND_SCRIPT)
            .key(&self.lock_key)
            .arg(&self.instance_id)
            .arg(new_ttl.as_millis() as u64)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to extend lock: {e}"))?;

        if extended == 1 {
            tracing::debug!(
                "Extended lock TTL for wallet {} to {:?}",
                self.wallet_address,
                new_ttl
            );
            Ok(true)
        } else {
            tracing::warn!(
                "Failed to extend lock for wallet {} - lock not held by this instance",
                self.wallet_address
            );
            Ok(false)
        }
    }
}

/// RAII guard that releases the lock when dropped
pub struct WalletLockGuard {
    conn: ConnectionManager,
    lock_key: String,
    instance_id: String,
    wallet_address: Address,
}

impl WalletLockGuard {
    /// Get the wallet address this lock is for
    pub fn wallet_address(&self) -> Address {
        self.wallet_address
    }

    /// Explicitly release the lock
    pub async fn release(self) -> Result<(), String> {
        self.release_internal().await
    }

    /// Internal release logic
    async fn release_internal(&self) -> Result<(), String> {
        let mut conn = self.conn.clone();

        // Lua script for atomic check-and-delete
        // Only delete if we still hold the lock
        let deleted: i32 = redis::Script::new(RELEASE_SCRIPT)
            .key(&self.lock_key)
            .arg(&self.instance_id)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to release lock: {e}"))?;

        if deleted == 1 {
            tracing::info!(
                "Released distributed lock for wallet {} (instance: {})",
                self.wallet_address,
                self.instance_id
            );
        } else {
            tracing::warn!(
                "Lock for wallet {} was already released or taken by another instance",
                self.wallet_address
            );
        }

        Ok(())
    }

    /// Extend the lock TTL
    pub async fn extend(&self, new_ttl: Duration) -> Result<bool, String> {
        let mut conn = self.conn.clone();

        let extended: i32 = redis::Script::new(EXTEND_SCRIPT)
            .key(&self.lock_key)
            .arg(&self.instance_id)
            .arg(new_ttl.as_millis() as u64)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to extend lock: {e}"))?;

        Ok(extended == 1)
    }

    /// Spawn a background heartbeat that extends this lock every `ttl / 3`.
    ///
    /// Long flows (USDC approval waits, modular recipes with several sequential
    /// transactions) hold a wallet far longer than the lock TTL. The heartbeat keeps
    /// the lock alive while the holder is making progress; if the lock is observed
    /// lost (TTL lapsed and another instance took it, or Redis stayed unreachable for
    /// a full TTL) the returned [`LockHeartbeat`] flips its `lock_lost` flag so the
    /// holder can abort before sending another transaction and risking a nonce
    /// collision.
    ///
    /// The heartbeat task is aborted when the returned [`LockHeartbeat`] is dropped —
    /// drop it BEFORE the guard so the lock cannot be re-extended after release.
    pub fn spawn_heartbeat(&self, ttl: Duration) -> LockHeartbeat {
        let mut conn = self.conn.clone();
        let lock_key = self.lock_key.clone();
        let instance_id = self.instance_id.clone();
        let wallet_address = self.wallet_address;
        let lost = Arc::new(AtomicBool::new(false));
        let lost_flag = Arc::clone(&lost);

        let interval = ttl / 3;
        let task = tokio::spawn(async move {
            // Allow up to a full TTL of consecutive Redis failures before declaring
            // the lock lost: 3 failed beats at ttl/3 spacing = the lock has expired.
            let mut consecutive_failures = 0u32;
            loop {
                tokio::time::sleep(interval).await;
                let extended: Result<i32, redis::RedisError> = redis::Script::new(EXTEND_SCRIPT)
                    .key(&lock_key)
                    .arg(&instance_id)
                    .arg(ttl.as_millis() as u64)
                    .invoke_async(&mut conn)
                    .await;
                match extended {
                    Ok(1) => {
                        consecutive_failures = 0;
                        tracing::trace!(
                            "Heartbeat extended lock for wallet {} by {:?}",
                            wallet_address,
                            ttl
                        );
                    }
                    Ok(_) => {
                        tracing::error!(
                            "Lock for wallet {} no longer held by this instance — flagging lock lost",
                            wallet_address
                        );
                        lost_flag.store(true, Ordering::SeqCst);
                        break;
                    }
                    Err(e) => {
                        consecutive_failures += 1;
                        tracing::warn!(
                            "Heartbeat extend failed for wallet {} ({} consecutive): {}",
                            wallet_address,
                            consecutive_failures,
                            e
                        );
                        if consecutive_failures >= 3 {
                            tracing::error!(
                                "Lock for wallet {} could not be extended for a full TTL — flagging lock lost",
                                wallet_address
                            );
                            lost_flag.store(true, Ordering::SeqCst);
                            break;
                        }
                    }
                }
            }
        });

        LockHeartbeat {
            task,
            lost,
            wallet_address,
        }
    }
}

/// Handle to a background lock-extension task (see [`WalletLockGuard::spawn_heartbeat`]).
///
/// Dropping this aborts the heartbeat task. Hold it for the lifetime of the lock and
/// drop it before (or together with, declared before) the lock guard so the heartbeat
/// can never keep a released lock alive.
pub struct LockHeartbeat {
    task: tokio::task::JoinHandle<()>,
    lost: Arc<AtomicBool>,
    wallet_address: Address,
}

impl LockHeartbeat {
    /// Whether the heartbeat observed the lock as lost.
    pub fn lock_lost(&self) -> bool {
        self.lost.load(Ordering::SeqCst)
    }

    /// Error if the lock was lost. Call this before every transaction send.
    pub fn ensure_held(&self) -> Result<(), String> {
        if self.lock_lost() {
            Err(format!(
                "wallet lock lost for {} — aborting to avoid nonce collision",
                self.wallet_address
            ))
        } else {
            Ok(())
        }
    }
}

impl Drop for LockHeartbeat {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Drop for WalletLockGuard {
    fn drop(&mut self) {
        // Only spawn release task if we have an active Tokio runtime
        // This prevents panics during shutdown when the runtime may not be available
        let mut conn = self.conn.clone();
        let lock_key = self.lock_key.clone();
        let instance_id = self.instance_id.clone();
        let wallet_address = self.wallet_address;

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move {
                    let result: Result<i32, _> = redis::Script::new(RELEASE_SCRIPT)
                        .key(&lock_key)
                        .arg(&instance_id)
                        .invoke_async(&mut conn)
                        .await;

                    match result {
                        Ok(1) => {
                            tracing::debug!("Lock released on drop for wallet {}", wallet_address)
                        }
                        Ok(_) => {
                            tracing::debug!("Lock already released for wallet {}", wallet_address)
                        }
                        Err(e) => tracing::error!(
                            "Failed to release lock on drop for wallet {}: {}",
                            wallet_address,
                            e
                        ),
                    }
                });
            }
            Err(_) => {
                // No Tokio runtime available (e.g., during shutdown)
                tracing::warn!(
                    "No Tokio runtime available to release lock for wallet {}",
                    wallet_address
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    // Note: These tests require a running Redis instance
    // Run with: cargo test --lib wallet -- --ignored

    async fn test_conn() -> ConnectionManager {
        let client = redis::Client::open("redis://127.0.0.1:6379").expect("Failed to open Redis");
        ConnectionManager::new(client)
            .await
            .expect("Failed to connect to Redis")
    }

    #[tokio::test]
    #[ignore = "requires Redis"]
    async fn test_beacon_update_lock_serializes_per_beacon() {
        let test_prefix = format!("test-{}:", uuid::Uuid::new_v4());
        let keys = PrefixedRedisKeys::new(&test_prefix);
        let conn = test_conn().await;

        let beacon_a = Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let beacon_b = Address::from_str("0x2222222222222222222222222222222222222222").unwrap();
        let ttl = Duration::from_secs(10);

        let lock_a = WalletLock::for_beacon_update(
            conn.clone(),
            beacon_a,
            "instance-1".to_string(),
            ttl,
            &keys,
        );
        let guard_a = lock_a
            .acquire(1, Duration::from_millis(50))
            .await
            .expect("first acquire for beacon A should succeed");

        // A second updater (different instance) must NOT get the same beacon.
        let lock_a2 = WalletLock::for_beacon_update(
            conn.clone(),
            beacon_a,
            "instance-2".to_string(),
            ttl,
            &keys,
        );
        assert!(
            lock_a2.try_acquire().await.is_err(),
            "concurrent update for the same beacon must be blocked"
        );

        // A DIFFERENT beacon is independent.
        let lock_b = WalletLock::for_beacon_update(
            conn.clone(),
            beacon_b,
            "instance-2".to_string(),
            ttl,
            &keys,
        );
        let guard_b = lock_b
            .try_acquire()
            .await
            .expect("different beacon must not be blocked");

        // The beacon namespace must not collide with the wallet lock for the
        // same address (a beacon lock must never block a pool-wallet lock).
        let wallet_lock_same_addr =
            WalletLock::with_keys(conn.clone(), beacon_a, "instance-3".to_string(), ttl, &keys);
        let guard_w = wallet_lock_same_addr
            .try_acquire()
            .await
            .expect("wallet lock namespace must be independent of beacon locks");

        guard_a.release().await.expect("release A");
        guard_b.release().await.expect("release B");
        guard_w.release().await.expect("release W");

        // After release, the beacon is acquirable again.
        let guard_a2 = lock_a2
            .try_acquire()
            .await
            .expect("beacon lock must be acquirable after release");
        guard_a2.release().await.expect("release A2");
    }

    #[tokio::test]
    #[ignore = "requires Redis"]
    async fn test_lock_acquire_release() {
        // Use unique prefix for test isolation
        let test_prefix = format!("test-{}:", uuid::Uuid::new_v4());
        let keys = PrefixedRedisKeys::new(&test_prefix);

        let conn = test_conn().await;
        let address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();

        let lock = WalletLock::with_keys(
            conn,
            address,
            "test-instance".to_string(),
            Duration::from_secs(10),
            &keys,
        );

        // Acquire lock
        let guard = lock
            .acquire(1, Duration::from_millis(100))
            .await
            .expect("Failed to acquire lock");

        // Check lock status
        assert!(lock.is_locked().await.expect("Failed to check lock"));
        assert_eq!(
            lock.get_holder().await.expect("Failed to get holder"),
            Some("test-instance".to_string())
        );

        // Release lock
        guard.release().await.expect("Failed to release lock");

        // Check lock is released
        assert!(!lock.is_locked().await.expect("Failed to check lock"));
    }

    #[tokio::test]
    #[ignore = "requires Redis"]
    async fn test_lock_contention() {
        // Use unique prefix for test isolation
        let test_prefix = format!("test-{}:", uuid::Uuid::new_v4());
        let keys = PrefixedRedisKeys::new(&test_prefix);

        let conn = test_conn().await;
        let address = Address::from_str("0x2234567890123456789012345678901234567890").unwrap();

        // Instance 1 acquires lock
        let lock1 = WalletLock::with_keys(
            conn.clone(),
            address,
            "instance-1".to_string(),
            Duration::from_secs(10),
            &keys,
        );
        let _guard1 = lock1
            .acquire(1, Duration::from_millis(100))
            .await
            .expect("Instance 1 should acquire lock");

        // Instance 2 tries to acquire - should fail
        let lock2 = WalletLock::with_keys(
            conn,
            address,
            "instance-2".to_string(),
            Duration::from_secs(10),
            &keys,
        );
        let result = lock2.try_acquire().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore = "requires Redis"]
    async fn test_lock_extend() {
        // Use unique prefix for test isolation
        let test_prefix = format!("test-{}:", uuid::Uuid::new_v4());
        let keys = PrefixedRedisKeys::new(&test_prefix);

        let conn = test_conn().await;
        let address = Address::from_str("0x3234567890123456789012345678901234567890").unwrap();

        let lock = WalletLock::with_keys(
            conn,
            address,
            "test-instance".to_string(),
            Duration::from_secs(5),
            &keys,
        );

        let guard = lock
            .acquire(1, Duration::from_millis(100))
            .await
            .expect("Failed to acquire lock");

        // Extend the lock
        let extended = guard
            .extend(Duration::from_secs(30))
            .await
            .expect("Failed to extend");
        assert!(extended);

        guard.release().await.expect("Failed to release lock");
    }

    #[tokio::test]
    #[ignore = "requires Redis"]
    async fn test_heartbeat_keeps_lock_alive_and_detects_loss() {
        let test_prefix = format!("test-{}:", uuid::Uuid::new_v4());
        let keys = PrefixedRedisKeys::new(&test_prefix);

        let conn = test_conn().await;
        let address = Address::from_str("0x4234567890123456789012345678901234567890").unwrap();

        let lock = WalletLock::with_keys(
            conn.clone(),
            address,
            "test-instance".to_string(),
            Duration::from_secs(2),
            &keys,
        );

        let guard = lock
            .acquire(1, Duration::from_millis(100))
            .await
            .expect("Failed to acquire lock");
        let heartbeat = guard.spawn_heartbeat(Duration::from_secs(2));

        // After 3s (longer than the 2s TTL) the heartbeat must have kept the lock alive
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert!(lock.is_locked().await.expect("Failed to check lock"));
        assert!(heartbeat.ensure_held().is_ok());

        // Steal the lock out from under the holder: delete + re-set as another instance
        let mut raw = conn.clone();
        let _: () = redis::cmd("SET")
            .arg(keys.wallet_lock(&address))
            .arg("other-instance")
            .arg("PX")
            .arg(60_000u64)
            .query_async(&mut raw)
            .await
            .expect("Failed to steal lock");

        // Next heartbeat tick observes the loss and flips the flag
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(heartbeat.lock_lost());
        assert!(heartbeat.ensure_held().is_err());

        drop(heartbeat);
        drop(guard);
    }
}
