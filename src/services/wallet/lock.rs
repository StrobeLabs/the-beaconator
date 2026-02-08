//! Distributed wallet locking using Redis
//!
//! Provides distributed locks to ensure only one beaconator instance
//! can use a wallet at a time, preventing nonce conflicts.

use alloy::primitives::Address;
use redis::AsyncCommands;
use std::time::Duration;

use crate::models::wallet::PrefixedRedisKeys;

/// A distributed lock for a specific wallet
pub struct WalletLock {
    redis: redis::Client,
    wallet_address: Address,
    instance_id: String,
    lock_key: String,
    ttl: Duration,
}

impl WalletLock {
    /// Create a new wallet lock with default "beaconator:" prefix
    pub fn new(
        redis: redis::Client,
        wallet_address: Address,
        instance_id: String,
        ttl: Duration,
    ) -> Self {
        Self::with_keys(
            redis,
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
        redis: redis::Client,
        wallet_address: Address,
        instance_id: String,
        ttl: Duration,
        keys: &PrefixedRedisKeys,
    ) -> Self {
        let lock_key = keys.wallet_lock(&wallet_address);
        Self {
            redis,
            wallet_address,
            instance_id,
            lock_key,
            ttl,
        }
    }

    /// Get a Redis connection
    async fn get_conn(&self) -> Result<redis::aio::MultiplexedConnection, String> {
        self.redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| format!("Redis connection failed: {e}"))
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
        let mut conn = self.get_conn().await?;

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
                    redis: self.redis.clone(),
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
        let mut conn = self.get_conn().await?;

        let exists: bool = conn
            .exists(&self.lock_key)
            .await
            .map_err(|e| format!("Failed to check lock status: {e}"))?;

        Ok(exists)
    }

    /// Get the current lock holder (if any)
    pub async fn get_holder(&self) -> Result<Option<String>, String> {
        let mut conn = self.get_conn().await?;

        let holder: Option<String> = conn
            .get(&self.lock_key)
            .await
            .map_err(|e| format!("Failed to get lock holder: {e}"))?;

        Ok(holder)
    }

    /// Extend the lock TTL (only if we hold the lock)
    pub async fn extend(&self, new_ttl: Duration) -> Result<bool, String> {
        let mut conn = self.get_conn().await?;

        // Lua script for atomic check-and-extend
        let script = r#"
            if redis.call("get", KEYS[1]) == ARGV[1] then
                return redis.call("pexpire", KEYS[1], ARGV[2])
            else
                return 0
            end
        "#;

        let extended: i32 = redis::Script::new(script)
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
    redis: redis::Client,
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
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| format!("Redis connection failed during lock release: {e}"))?;

        // Lua script for atomic check-and-delete
        // Only delete if we still hold the lock
        let script = r#"
            if redis.call("get", KEYS[1]) == ARGV[1] then
                return redis.call("del", KEYS[1])
            else
                return 0
            end
        "#;

        let deleted: i32 = redis::Script::new(script)
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
        let mut conn = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| format!("Redis connection failed: {e}"))?;

        let script = r#"
            if redis.call("get", KEYS[1]) == ARGV[1] then
                return redis.call("pexpire", KEYS[1], ARGV[2])
            else
                return 0
            end
        "#;

        let extended: i32 = redis::Script::new(script)
            .key(&self.lock_key)
            .arg(&self.instance_id)
            .arg(new_ttl.as_millis() as u64)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to extend lock: {e}"))?;

        Ok(extended == 1)
    }
}

impl Drop for WalletLockGuard {
    fn drop(&mut self) {
        // Only spawn release task if we have an active Tokio runtime
        // This prevents panics during shutdown when the runtime may not be available
        let redis = self.redis.clone();
        let lock_key = self.lock_key.clone();
        let instance_id = self.instance_id.clone();
        let wallet_address = self.wallet_address;

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move {
                    if let Ok(mut conn) = redis.get_multiplexed_async_connection().await {
                        let script = r#"
                            if redis.call("get", KEYS[1]) == ARGV[1] then
                                return redis.call("del", KEYS[1])
                            else
                                return 0
                            end
                        "#;

                        let result: Result<i32, _> = redis::Script::new(script)
                            .key(&lock_key)
                            .arg(&instance_id)
                            .invoke_async(&mut conn)
                            .await;

                        match result {
                            Ok(1) => {
                                tracing::debug!(
                                    "Lock released on drop for wallet {}",
                                    wallet_address
                                )
                            }
                            Ok(_) => {
                                tracing::debug!(
                                    "Lock already released for wallet {}",
                                    wallet_address
                                )
                            }
                            Err(e) => tracing::error!(
                                "Failed to release lock on drop for wallet {}: {}",
                                wallet_address,
                                e
                            ),
                        }
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

    #[tokio::test]
    #[ignore = "requires Redis"]
    async fn test_lock_acquire_release() {
        // Use unique prefix for test isolation
        let test_prefix = format!("test-{}:", uuid::Uuid::new_v4());
        let keys = PrefixedRedisKeys::new(&test_prefix);

        let redis = redis::Client::open("redis://127.0.0.1:6379").expect("Failed to open Redis");
        let address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();

        let lock = WalletLock::with_keys(
            redis,
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

        let redis = redis::Client::open("redis://127.0.0.1:6379").expect("Failed to open Redis");
        let address = Address::from_str("0x2234567890123456789012345678901234567890").unwrap();

        // Instance 1 acquires lock
        let lock1 = WalletLock::with_keys(
            redis.clone(),
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
            redis,
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

        let redis = redis::Client::open("redis://127.0.0.1:6379").expect("Failed to open Redis");
        let address = Address::from_str("0x3234567890123456789012345678901234567890").unwrap();

        let lock = WalletLock::with_keys(
            redis,
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
}
