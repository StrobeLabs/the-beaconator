//! Redis-backed wallet pool management
//!
//! Manages a pool of Turnkey wallets stored in Redis, allowing multiple
//! beaconator instances to share wallets safely.

use alloy::primitives::Address;
use redis::AsyncCommands;
use std::str::FromStr;

use crate::models::wallet::{RedisKeys, WalletInfo, WalletStatus};

/// Redis-backed wallet pool
pub struct WalletPool {
    redis: redis::Client,
    instance_id: String,
}

impl WalletPool {
    /// Create a new wallet pool
    pub async fn new(redis_url: &str, instance_id: String) -> Result<Self, String> {
        let redis = redis::Client::open(redis_url)
            .map_err(|e| format!("Failed to connect to Redis: {e}"))?;

        // Test connection
        let mut conn = redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| format!("Failed to get Redis connection: {e}"))?;

        let _: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| format!("Redis ping failed: {e}"))?;

        tracing::info!("Wallet pool connected to Redis");

        Ok(Self { redis, instance_id })
    }

    /// Get a Redis connection
    async fn get_conn(&self) -> Result<redis::aio::MultiplexedConnection, String> {
        self.redis
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| format!("Redis connection failed: {e}"))
    }

    /// Get the instance ID
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    /// Get the Redis client (for creating locks)
    pub fn redis_client(&self) -> &redis::Client {
        &self.redis
    }

    /// List all wallets in the pool
    pub async fn list_wallets(&self) -> Result<Vec<WalletInfo>, String> {
        let mut conn = self.get_conn().await?;

        let addresses: Vec<String> = conn
            .smembers(RedisKeys::wallet_pool())
            .await
            .map_err(|e| format!("Failed to list wallets: {e}"))?;

        let mut wallets = Vec::new();
        for addr_str in addresses {
            if let Ok(address) = Address::from_str(&addr_str)
                && let Ok(info) = self.get_wallet_info(&address).await
            {
                wallets.push(info);
            }
        }

        Ok(wallets)
    }

    /// List all available (not locked) wallets
    pub async fn list_available_wallets(&self) -> Result<Vec<WalletInfo>, String> {
        let wallets = self.list_wallets().await?;
        Ok(wallets
            .into_iter()
            .filter(|w| matches!(w.status, WalletStatus::Available))
            .collect())
    }

    /// Add a wallet to the pool
    pub async fn add_wallet(&self, info: WalletInfo) -> Result<(), String> {
        let mut conn = self.get_conn().await?;

        // Add to pool set
        let _: () = conn
            .sadd(RedisKeys::wallet_pool(), info.address.to_string())
            .await
            .map_err(|e| format!("Failed to add wallet to pool: {e}"))?;

        // Store wallet info as JSON
        let info_json = serde_json::to_string(&info)
            .map_err(|e| format!("Failed to serialize wallet info: {e}"))?;

        let _: () = conn
            .set(RedisKeys::wallet_info(&info.address), info_json)
            .await
            .map_err(|e| format!("Failed to store wallet info: {e}"))?;

        tracing::info!("Added wallet {} to pool", info.address);

        Ok(())
    }

    /// Get wallet info by address
    pub async fn get_wallet_info(&self, address: &Address) -> Result<WalletInfo, String> {
        let mut conn = self.get_conn().await?;

        let info_json: Option<String> = conn
            .get(RedisKeys::wallet_info(address))
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
        let mut conn = self.get_conn().await?;

        let info_json = serde_json::to_string(info)
            .map_err(|e| format!("Failed to serialize wallet info: {e}"))?;

        let _: () = conn
            .set(RedisKeys::wallet_info(&info.address), info_json)
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
    /// that were designated to this wallet.
    pub async fn remove_wallet(&self, address: &Address) -> Result<(), String> {
        let mut conn = self.get_conn().await?;

        // First, get all beacons designated to this wallet and remove reverse mappings
        let beacon_strs: Vec<String> = conn
            .smembers(RedisKeys::wallet_beacons(address))
            .await
            .map_err(|e| format!("Failed to get beacons for wallet: {e}"))?;

        for beacon_str in beacon_strs {
            if let Ok(beacon_addr) = Address::from_str(&beacon_str) {
                let _: () = conn
                    .del(RedisKeys::beacon_wallet(&beacon_addr))
                    .await
                    .map_err(|e| {
                        format!("Failed to remove beacon->wallet mapping for {beacon_addr}: {e}")
                    })?;
            }
        }

        // Remove from pool set
        let _: () = conn
            .srem(RedisKeys::wallet_pool(), address.to_string())
            .await
            .map_err(|e| format!("Failed to remove wallet from pool: {e}"))?;

        // Remove wallet info
        let _: () = conn
            .del(RedisKeys::wallet_info(address))
            .await
            .map_err(|e| format!("Failed to remove wallet info: {e}"))?;

        // Remove the wallet's beacon set
        let _: () = conn
            .del(RedisKeys::wallet_beacons(address))
            .await
            .map_err(|e| format!("Failed to remove wallet beacon mappings: {e}"))?;

        tracing::info!("Removed wallet {} from pool", address);

        Ok(())
    }

    /// Check if a wallet exists in the pool
    pub async fn wallet_exists(&self, address: &Address) -> Result<bool, String> {
        let mut conn = self.get_conn().await?;

        let exists: bool = conn
            .sismember(RedisKeys::wallet_pool(), address.to_string())
            .await
            .map_err(|e| format!("Failed to check wallet existence: {e}"))?;

        Ok(exists)
    }

    /// Get the number of wallets in the pool
    pub async fn wallet_count(&self) -> Result<usize, String> {
        let mut conn = self.get_conn().await?;

        let count: usize = conn
            .scard(RedisKeys::wallet_pool())
            .await
            .map_err(|e| format!("Failed to count wallets: {e}"))?;

        Ok(count)
    }

    /// Get the first available wallet (not locked)
    pub async fn get_available_wallet(&self) -> Result<Option<WalletInfo>, String> {
        let available = self.list_available_wallets().await?;
        Ok(available.into_iter().next())
    }

    /// Add a beacon to a wallet's designated beacons list
    pub async fn add_designated_beacon(
        &self,
        wallet_address: &Address,
        beacon_address: &Address,
    ) -> Result<(), String> {
        let mut conn = self.get_conn().await?;

        // Add to wallet's beacon set
        let _: () = conn
            .sadd(
                RedisKeys::wallet_beacons(wallet_address),
                beacon_address.to_string(),
            )
            .await
            .map_err(|e| format!("Failed to add beacon to wallet: {e}"))?;

        // Add reverse mapping: beacon -> wallet
        let _: () = conn
            .set(
                RedisKeys::beacon_wallet(beacon_address),
                wallet_address.to_string(),
            )
            .await
            .map_err(|e| format!("Failed to set beacon wallet mapping: {e}"))?;

        // Update wallet info to include this beacon
        let mut info = self.get_wallet_info(wallet_address).await?;
        if !info.designated_beacons.contains(beacon_address) {
            info.designated_beacons.push(*beacon_address);
            self.update_wallet_info(&info).await?;
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
        let mut conn = self.get_conn().await?;

        let wallet_str: Option<String> =
            conn.get(RedisKeys::beacon_wallet(beacon_address))
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
        let mut conn = self.get_conn().await?;

        let beacon_strs: Vec<String> = conn
            .smembers(RedisKeys::wallet_beacons(wallet_address))
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
    pub async fn remove_designated_beacon(
        &self,
        wallet_address: &Address,
        beacon_address: &Address,
    ) -> Result<(), String> {
        let mut conn = self.get_conn().await?;

        // Remove from wallet's beacon set
        let _: () = conn
            .srem(
                RedisKeys::wallet_beacons(wallet_address),
                beacon_address.to_string(),
            )
            .await
            .map_err(|e| format!("Failed to remove beacon from wallet: {e}"))?;

        // Remove reverse mapping
        let _: () = conn
            .del(RedisKeys::beacon_wallet(beacon_address))
            .await
            .map_err(|e| format!("Failed to remove beacon wallet mapping: {e}"))?;

        // Update wallet info
        let mut info = self.get_wallet_info(wallet_address).await?;
        info.designated_beacons.retain(|b| b != beacon_address);
        self.update_wallet_info(&info).await?;

        tracing::info!(
            "Removed beacon {} from wallet {} designated beacons",
            beacon_address,
            wallet_address
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    // Note: These tests require a running Redis instance
    // Run with: cargo test --lib wallet -- --ignored

    #[tokio::test]
    #[serial]
    #[ignore = "requires Redis"]
    async fn test_wallet_pool_operations() {
        let pool = WalletPool::new("redis://127.0.0.1:6379", "test-instance".to_string())
            .await
            .expect("Failed to create pool");

        let address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
        let info = WalletInfo {
            address,
            turnkey_key_id: "key-123".to_string(),
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
        assert_eq!(retrieved.turnkey_key_id, "key-123");

        // Count
        let count = pool.wallet_count().await.expect("Failed to count");
        assert!(count >= 1);

        // Remove
        pool.remove_wallet(&address)
            .await
            .expect("Failed to remove");
        assert!(!pool.wallet_exists(&address).await.expect("Failed to check"));
    }
}
