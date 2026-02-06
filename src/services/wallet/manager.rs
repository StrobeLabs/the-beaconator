//! Central wallet management coordinator
//!
//! This module provides the WalletManager, which coordinates wallet pool
//! operations, locking, and beacon mappings into a unified interface.

use super::{TurnkeySigner, WalletLock, WalletLockGuard, WalletPool};
use alloy::primitives::Address;
use alloy::signers::Signer;

use crate::models::wallet::WalletManagerConfig;

/// A handle to a locked wallet ready for use
///
/// This combines the signer with its lock guard, ensuring the wallet
/// remains locked for the duration of operations.
pub struct WalletHandle {
    /// The Turnkey signer for this wallet
    pub signer: TurnkeySigner,
    /// The lock guard - wallet is locked until this is dropped
    pub lock_guard: WalletLockGuard,
}

impl WalletHandle {
    /// Get the Ethereum address of this wallet
    pub fn address(&self) -> Address {
        self.signer.address()
    }
}

/// Central coordinator for wallet operations
///
/// The WalletManager provides a high-level interface for:
/// - Acquiring wallets for operations (with automatic locking)
/// - Managing the wallet pool
/// - Looking up beacon-to-wallet mappings
pub struct WalletManager {
    /// The wallet pool
    pool: WalletPool,
    /// Configuration
    config: WalletManagerConfig,
}

impl WalletManager {
    /// Create a new WalletManager
    ///
    /// # Arguments
    /// * `config` - Configuration for the wallet manager
    pub async fn new(config: WalletManagerConfig) -> Result<Self, String> {
        let instance_id = config
            .instance_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let pool = WalletPool::new(&config.redis_url, instance_id).await?;

        Ok(Self { pool, config })
    }

    /// Acquire a wallet for a beacon update operation
    ///
    /// If the beacon has a designated wallet, that wallet will be used.
    /// Otherwise, an available wallet from the pool will be acquired.
    ///
    /// # Arguments
    /// * `beacon` - The beacon that needs to be updated
    ///
    /// # Returns
    /// A WalletHandle with the locked wallet ready for use
    pub async fn acquire_for_beacon(&self, beacon: &Address) -> Result<WalletHandle, String> {
        // Check if beacon has a designated wallet
        if let Some(wallet_address) = self.pool.get_wallet_for_beacon(beacon).await? {
            self.acquire_specific_wallet(&wallet_address).await
        } else {
            self.acquire_any_wallet().await
        }
    }

    /// Acquire a specific wallet by address
    ///
    /// # Arguments
    /// * `address` - The wallet address to acquire
    pub async fn acquire_specific_wallet(&self, address: &Address) -> Result<WalletHandle, String> {
        // Get wallet info from pool
        let wallet_info = self.pool.get_wallet_info(address).await?;

        // Create and acquire lock
        let lock = WalletLock::new(
            self.pool.redis_client().clone(),
            *address,
            self.pool.instance_id().to_string(),
            self.config.lock_ttl,
        );

        let lock_guard = lock
            .acquire(self.config.lock_retry_count, self.config.lock_retry_delay)
            .await?;

        // Create signer
        let signer = TurnkeySigner::new(
            self.config.turnkey_api_url.clone(),
            self.config.turnkey_organization_id.clone(),
            self.config.turnkey_api_public_key.clone(),
            self.config.turnkey_api_private_key.clone(),
            wallet_info.turnkey_key_id.clone(),
            wallet_info.address,
            self.config.chain_id,
        )
        .map_err(|e| format!("Failed to create Turnkey signer: {e}"))?;

        Ok(WalletHandle { signer, lock_guard })
    }

    /// Acquire any available wallet from the pool
    pub async fn acquire_any_wallet(&self) -> Result<WalletHandle, String> {
        let available = self.pool.list_available_wallets().await?;

        if available.is_empty() {
            return Err("No available wallets in the pool".to_string());
        }

        // Try to acquire each available wallet until one succeeds
        for wallet_info in available {
            let lock = WalletLock::new(
                self.pool.redis_client().clone(),
                wallet_info.address,
                self.pool.instance_id().to_string(),
                self.config.lock_ttl,
            );

            match lock
                .acquire(self.config.lock_retry_count, self.config.lock_retry_delay)
                .await
            {
                Ok(lock_guard) => {
                    let signer = TurnkeySigner::new(
                        self.config.turnkey_api_url.clone(),
                        self.config.turnkey_organization_id.clone(),
                        self.config.turnkey_api_public_key.clone(),
                        self.config.turnkey_api_private_key.clone(),
                        wallet_info.turnkey_key_id.clone(),
                        wallet_info.address,
                        self.config.chain_id,
                    )
                    .map_err(|e| format!("Failed to create Turnkey signer: {e}"))?;

                    return Ok(WalletHandle { signer, lock_guard });
                }
                Err(_) => continue, // Try next wallet
            }
        }

        Err("Failed to acquire any wallet from the pool".to_string())
    }

    /// Get access to the wallet pool
    pub fn pool(&self) -> &WalletPool {
        &self.pool
    }

    /// Get the instance ID
    pub fn instance_id(&self) -> &str {
        self.pool.instance_id()
    }

    /// Create a lock for a specific wallet
    ///
    /// This is useful when you need to manage the lock separately from
    /// acquiring a wallet handle.
    pub fn create_lock(&self, address: &Address) -> WalletLock {
        WalletLock::new(
            self.pool.redis_client().clone(),
            *address,
            self.pool.instance_id().to_string(),
            self.config.lock_ttl,
        )
    }
}
