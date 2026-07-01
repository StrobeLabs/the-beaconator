//! Redis-backed wallet pool management
//!
//! Manages a pool of wallets stored in Redis, allowing multiple
//! beaconator instances to share wallets safely.

use alloy::primitives::Address;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use std::str::FromStr;

use crate::models::wallet::{PrefixedRedisKeys, WalletInfo, WalletStatus};

/// Redis-backed wallet pool
pub struct WalletPool {
    conn: ConnectionManager,
    instance_id: String,
    keys: PrefixedRedisKeys,
}

impl WalletPool {
    /// Create a new wallet pool with the default "beaconator:" prefix
    pub async fn new(redis_url: &str, instance_id: String) -> Result<Self, String> {
        Self::with_prefix(redis_url, instance_id, "beaconator:").await
    }

    /// Create a new wallet pool with a custom prefix
    ///
    /// This is useful for test isolation - each test can use a unique prefix
    /// to avoid conflicts when running tests in parallel.
    pub async fn with_prefix(
        redis_url: &str,
        instance_id: String,
        prefix: &str,
    ) -> Result<Self, String> {
        let redis = redis::Client::open(redis_url)
            .map_err(|e| format!("Failed to connect to Redis: {e}"))?;

        // One auto-reconnecting connection per pool; cloned per operation. This
        // avoids opening a fresh (TLS) connection for every Redis command.
        let mut conn = ConnectionManager::new(redis)
            .await
            .map_err(|e| format!("Failed to get Redis connection: {e}"))?;

        let _: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Redis ping failed: {e}"))?;

        tracing::info!("Wallet pool connected to Redis with prefix '{}'", prefix);

        Ok(Self {
            conn,
            instance_id,
            keys: PrefixedRedisKeys::new(prefix),
        })
    }

    /// Get a Redis connection (cheap clone of the shared auto-reconnecting manager)
    fn get_conn(&self) -> ConnectionManager {
        self.conn.clone()
    }

    /// Get the instance ID
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    /// Get the shared Redis connection manager (for creating locks)
    pub fn connection(&self) -> &ConnectionManager {
        &self.conn
    }

    /// Get the Redis key generator (for creating locks with matching prefix)
    pub fn keys(&self) -> &PrefixedRedisKeys {
        &self.keys
    }

    /// List all wallets in the pool
    pub async fn list_wallets(&self) -> Result<Vec<WalletInfo>, String> {
        let mut conn = self.get_conn();

        let addresses: Vec<String> = conn
            .smembers(self.keys.wallet_pool())
            .await
            .map_err(|e| format!("Failed to list wallets: {e}"))?;

        // Single MGET instead of one GET per wallet (avoids N+1 round trips).
        let info_keys: Vec<String> = addresses
            .iter()
            .filter_map(|addr_str| Address::from_str(addr_str).ok())
            .map(|address| self.keys.wallet_info(&address))
            .collect();

        if info_keys.is_empty() {
            return Ok(Vec::new());
        }

        let info_jsons: Vec<Option<String>> = conn
            .mget(&info_keys)
            .await
            .map_err(|e| format!("Failed to fetch wallet info: {e}"))?;

        let wallets = info_jsons
            .into_iter()
            .flatten()
            .filter_map(|json| serde_json::from_str::<WalletInfo>(&json).ok())
            .collect();

        Ok(wallets)
    }

    /// List all wallets eligible for acquisition.
    ///
    /// NOTE: the `WalletInfo.status` field is informational only — nothing in
    /// production ever sets `Locked` (locking is enforced through the Redis lock
    /// keys, not this field), so wallets are listed regardless of status.
    pub async fn list_available_wallets(&self) -> Result<Vec<WalletInfo>, String> {
        self.list_wallets().await
    }

    /// Add a wallet to the pool
    ///
    /// Uses an atomic Redis pipeline to ensure both operations succeed or fail together.
    pub async fn add_wallet(&self, info: WalletInfo) -> Result<(), String> {
        let mut conn = self.get_conn();

        // Serialize wallet info
        let info_json = serde_json::to_string(&info)
            .map_err(|e| format!("Failed to serialize wallet info: {e}"))?;

        // Use atomic pipeline to add wallet to pool set and store wallet info
        let _: () = redis::pipe()
            .atomic()
            .sadd(self.keys.wallet_pool(), info.address.to_string())
            .set(self.keys.wallet_info(&info.address), info_json)
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to add wallet to pool: {e}"))?;

        tracing::info!("Added wallet {} to pool", info.address);

        Ok(())
    }

    /// Get wallet info by address
    pub async fn get_wallet_info(&self, address: &Address) -> Result<WalletInfo, String> {
        let mut conn = self.get_conn();

        let info_json: Option<String> = conn
            .get(self.keys.wallet_info(address))
            .await
            .map_err(|e| format!("Failed to get wallet info: {e}"))?;

        match info_json {
            Some(json) => serde_json::from_str(&json)
                .map_err(|e| format!("Failed to deserialize wallet info: {e}")),
            None => Err(format!("Wallet {address} not found in pool")),
        }
    }

    /// Update wallet info
    pub async fn update_wallet_info(&self, info: &WalletInfo) -> Result<(), String> {
        let mut conn = self.get_conn();

        let info_json = serde_json::to_string(info)
            .map_err(|e| format!("Failed to serialize wallet info: {e}"))?;

        let _: () = conn
            .set(self.keys.wallet_info(&info.address), info_json)
            .await
            .map_err(|e| format!("Failed to update wallet info: {e}"))?;

        Ok(())
    }

    /// Update wallet status
    pub async fn update_wallet_status(
        &self,
        address: &Address,
        status: WalletStatus,
    ) -> Result<(), String> {
        let mut info = self.get_wallet_info(address).await?;
        info.status = status;
        self.update_wallet_info(&info).await
    }

    /// Remove a wallet from the pool
    ///
    /// This also cleans up all beacon→wallet reverse mappings for beacons
    /// that were designated to this wallet. Uses an atomic Redis pipeline
    /// to ensure all operations succeed or fail together.
    pub async fn remove_wallet(&self, address: &Address) -> Result<(), String> {
        let mut conn = self.get_conn();

        // First, get all beacons designated to this wallet
        let beacon_strs: Vec<String> = conn
            .smembers(self.keys.wallet_beacons(address))
            .await
            .map_err(|e| format!("Failed to get beacons for wallet: {e}"))?;

        // Build atomic pipeline for all deletions
        let mut pipe = redis::pipe();
        pipe.atomic();

        // Add beacon->wallet reverse mapping deletions to pipeline
        for beacon_str in &beacon_strs {
            if let Ok(beacon_addr) = Address::from_str(beacon_str) {
                pipe.del(self.keys.beacon_wallet(&beacon_addr));
            }
        }

        // Add wallet pool removal
        pipe.srem(self.keys.wallet_pool(), address.to_string());

        // Add wallet info deletion
        pipe.del(self.keys.wallet_info(address));

        // Add wallet beacons set deletion
        pipe.del(self.keys.wallet_beacons(address));

        // Drop the wallet's LRU score so a re-added address sorts as
        // never-touched instead of resurfacing with a stale score.
        pipe.zrem(self.keys.wallet_lru(), address.to_string());

        // Execute all deletions atomically
        let _: () = pipe
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to remove wallet from pool: {e}"))?;

        tracing::info!("Removed wallet {} from pool", address);

        Ok(())
    }

    /// Check if a wallet exists in the pool
    pub async fn wallet_exists(&self, address: &Address) -> Result<bool, String> {
        let mut conn = self.get_conn();

        let exists: bool = conn
            .sismember(self.keys.wallet_pool(), address.to_string())
            .await
            .map_err(|e| format!("Failed to check wallet existence: {e}"))?;

        Ok(exists)
    }

    /// Get the number of wallets in the pool
    pub async fn wallet_count(&self) -> Result<usize, String> {
        let mut conn = self.get_conn();

        let count: usize = conn
            .scard(self.keys.wallet_pool())
            .await
            .map_err(|e| format!("Failed to count wallets: {e}"))?;

        Ok(count)
    }

    /// Get the first available wallet (not locked)
    pub async fn get_available_wallet(&self) -> Result<Option<WalletInfo>, String> {
        let available = self.list_available_wallets().await?;
        Ok(available.into_iter().next())
    }

    /// Read the wallet LRU order: addresses sorted ascending by last-acquired
    /// score (oldest / never-acquired first). Used to spread selection across
    /// the pool instead of hammering whichever wallet sorts first in Redis.
    pub async fn lru_order(&self) -> Result<Vec<Address>, String> {
        let mut conn = self.get_conn();

        let members: Vec<String> = conn
            .zrange(self.keys.wallet_lru(), 0, -1)
            .await
            .map_err(|e| format!("Failed to read wallet LRU order: {e}"))?;

        Ok(members
            .iter()
            .filter_map(|addr_str| Address::from_str(addr_str).ok())
            .collect())
    }

    /// Record that `address` was just acquired, moving it to the back of the
    /// LRU order (least-recently-used selection prefers it last next time).
    pub async fn touch_lru(&self, address: &Address) -> Result<(), String> {
        let mut conn = self.get_conn();

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("System time error: {e}"))?
            .as_millis() as i64;

        let _: () = conn
            .zadd(self.keys.wallet_lru(), address.to_string(), now_ms)
            .await
            .map_err(|e| format!("Failed to touch wallet LRU entry: {e}"))?;

        Ok(())
    }

    /// Add a beacon to a wallet's designated beacons list
    ///
    /// Uses an atomic Redis pipeline for the key operations, then updates
    /// the wallet info separately to maintain the denormalized data.
    pub async fn add_designated_beacon(
        &self,
        wallet_address: &Address,
        beacon_address: &Address,
    ) -> Result<(), String> {
        let mut conn = self.get_conn();

        // Use atomic pipeline for the two Redis key operations
        let _: () = redis::pipe()
            .atomic()
            .sadd(
                self.keys.wallet_beacons(wallet_address),
                beacon_address.to_string(),
            )
            .set(
                self.keys.beacon_wallet(beacon_address),
                wallet_address.to_string(),
            )
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to add beacon mapping: {e}"))?;

        // Update wallet info to include this beacon (denormalized data for convenience)
        // This is done separately from the atomic Redis operations
        match self.get_wallet_info(wallet_address).await {
            Ok(mut info) => {
                if !info.designated_beacons.contains(beacon_address) {
                    info.designated_beacons.push(*beacon_address);
                    if let Err(e) = self.update_wallet_info(&info).await {
                        // Log but don't fail - the authoritative mappings are in Redis keys
                        tracing::warn!(
                            "Failed to update wallet info for {}: {} (Redis mappings are intact)",
                            wallet_address,
                            e
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Could not get wallet info for {} to update designated beacons: {}",
                    wallet_address,
                    e
                );
            }
        }

        tracing::info!(
            "Added beacon {} to wallet {} designated beacons",
            beacon_address,
            wallet_address
        );

        Ok(())
    }

    /// Get the wallet designated for a specific beacon
    pub async fn get_wallet_for_beacon(
        &self,
        beacon_address: &Address,
    ) -> Result<Option<Address>, String> {
        let mut conn = self.get_conn();

        let wallet_str: Option<String> = conn
            .get(self.keys.beacon_wallet(beacon_address))
            .await
            .map_err(|e| format!("Failed to get wallet for beacon: {e}"))?;

        match wallet_str {
            Some(addr) => {
                let address = Address::from_str(&addr)
                    .map_err(|e| format!("Invalid wallet address in beacon mapping: {e}"))?;
                Ok(Some(address))
            }
            None => Ok(None),
        }
    }

    /// Get all beacons designated to a wallet
    pub async fn get_beacons_for_wallet(
        &self,
        wallet_address: &Address,
    ) -> Result<Vec<Address>, String> {
        let mut conn = self.get_conn();

        let beacon_strs: Vec<String> = conn
            .smembers(self.keys.wallet_beacons(wallet_address))
            .await
            .map_err(|e| format!("Failed to get beacons for wallet: {e}"))?;

        let mut beacons = Vec::new();
        for addr_str in beacon_strs {
            if let Ok(address) = Address::from_str(&addr_str) {
                beacons.push(address);
            }
        }

        Ok(beacons)
    }

    /// Remove a beacon from a wallet's designated beacons list
    ///
    /// Uses an atomic Redis pipeline for the key operations, then updates
    /// the wallet info separately to maintain the denormalized data.
    pub async fn remove_designated_beacon(
        &self,
        wallet_address: &Address,
        beacon_address: &Address,
    ) -> Result<(), String> {
        let mut conn = self.get_conn();

        // Use atomic pipeline for the two Redis key operations
        let _: () = redis::pipe()
            .atomic()
            .srem(
                self.keys.wallet_beacons(wallet_address),
                beacon_address.to_string(),
            )
            .del(self.keys.beacon_wallet(beacon_address))
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to remove beacon mapping: {e}"))?;

        // Update wallet info to remove this beacon (denormalized data for convenience)
        // This is done separately from the atomic Redis operations
        match self.get_wallet_info(wallet_address).await {
            Ok(mut info) => {
                info.designated_beacons.retain(|b| b != beacon_address);
                if let Err(e) = self.update_wallet_info(&info).await {
                    // Log but don't fail - the authoritative mappings are in Redis keys
                    tracing::warn!(
                        "Failed to update wallet info for {}: {} (Redis mappings are intact)",
                        wallet_address,
                        e
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Could not get wallet info for {} to update designated beacons: {}",
                    wallet_address,
                    e
                );
            }
        }

        tracing::info!(
            "Removed beacon {} from wallet {} designated beacons",
            beacon_address,
            wallet_address
        );

        Ok(())
    }

    /// Clean up all Redis keys with this pool's prefix
    ///
    /// This is useful for test teardown to remove all keys created during a test.
    /// WARNING: This will delete ALL keys matching the prefix pattern.
    pub async fn cleanup(&self) -> Result<(), String> {
        let mut conn = self.get_conn();
        let pattern = format!("{}*", self.keys.prefix());

        // Use KEYS to find all keys with our prefix
        // Note: In production with large datasets, SCAN would be preferred
        // but for test cleanup, KEYS is simpler and sufficient
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Failed to scan keys: {e}"))?;

        if !keys.is_empty() {
            tracing::debug!(
                "Cleaning up {} Redis keys with prefix '{}'",
                keys.len(),
                self.keys.prefix()
            );
            let _: () = redis::cmd("DEL")
                .arg(&keys)
                .query_async(&mut conn)
                .await
                .map_err(|e| format!("Failed to delete keys: {e}"))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a running Redis instance
    // Run with: cargo test --lib wallet -- --ignored

    #[tokio::test]
    #[ignore = "requires Redis"]
    async fn test_wallet_pool_operations() {
        // Use unique prefix for test isolation
        let test_prefix = format!("test-{}:", uuid::Uuid::new_v4());
        let pool = WalletPool::with_prefix(
            "redis://127.0.0.1:6379",
            "test-instance".to_string(),
            &test_prefix,
        )
        .await
        .expect("Failed to create pool");

        let address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
        let info = WalletInfo {
            address,
            key_id: "key-123".to_string(),
            status: WalletStatus::Available,
            designated_beacons: vec![],
        };

        // Add wallet
        pool.add_wallet(info.clone())
            .await
            .expect("Failed to add wallet");

        // Check exists
        assert!(pool.wallet_exists(&address).await.expect("Failed to check"));

        // Get info
        let retrieved = pool.get_wallet_info(&address).await.expect("Failed to get");
        assert_eq!(retrieved.address, address);
        assert_eq!(retrieved.key_id, "key-123");

        // Count
        let count = pool.wallet_count().await.expect("Failed to count");
        assert_eq!(count, 1); // Exact count since we have isolated prefix

        // Remove
        pool.remove_wallet(&address)
            .await
            .expect("Failed to remove");
        assert!(!pool.wallet_exists(&address).await.expect("Failed to check"));

        // Cleanup test keys
        pool.cleanup().await.expect("Failed to cleanup");
    }
}
