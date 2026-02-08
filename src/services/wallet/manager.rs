//! Central wallet management coordinator
//!
//! This module provides the WalletManager, which coordinates wallet pool
//! operations, locking, and beacon mappings into a unified interface.

use std::collections::HashMap;

use super::{TurnkeySigner, WalletLock, WalletLockGuard, WalletPool};
use alloy::network::EthereumWallet;
use alloy::primitives::{Address, B256};
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::{Error as SignerError, Signature, Signer};

use crate::AlloyProvider;
use crate::models::wallet::{WalletInfo, WalletManagerConfig};

/// Signer type that supports both Turnkey and local private key signers
#[derive(Clone)]
pub enum WalletSigner {
    /// Turnkey API-backed signer for production
    Turnkey(TurnkeySigner),
    /// Local private key signer for testing
    Local(PrivateKeySigner),
}

impl WalletSigner {
    /// Get the address of the signer
    pub fn address(&self) -> Address {
        match self {
            WalletSigner::Turnkey(s) => s.address(),
            WalletSigner::Local(s) => s.address(),
        }
    }

    /// Sign a hash using the underlying signer
    pub async fn sign_hash(&self, hash: &B256) -> Result<Signature, SignerError> {
        match self {
            WalletSigner::Turnkey(s) => s.sign_hash(hash).await,
            WalletSigner::Local(s) => s.sign_hash(hash).await,
        }
    }
}

/// A handle to a locked wallet ready for use
///
/// This combines the signer with its lock guard, ensuring the wallet
/// remains locked for the duration of operations.
pub struct WalletHandle {
    /// The signer for this wallet (Turnkey or local)
    pub signer: WalletSigner,
    /// The lock guard - wallet is locked until this is dropped
    pub lock_guard: WalletLockGuard,
}

impl WalletHandle {
    /// Get the Ethereum address of this wallet
    pub fn address(&self) -> Address {
        self.signer.address()
    }

    /// Build an AlloyProvider using this wallet's signer
    ///
    /// Creates a provider that can sign transactions using the wallet.
    ///
    /// # Arguments
    /// * `rpc_url` - The RPC URL to connect to
    ///
    /// # Returns
    /// An AlloyProvider configured with this wallet's signer
    pub fn build_provider(&self, rpc_url: &str) -> Result<AlloyProvider, String> {
        let wallet = match &self.signer {
            WalletSigner::Turnkey(s) => EthereumWallet::from(s.clone()),
            WalletSigner::Local(s) => EthereumWallet::from(s.clone()),
        };

        let provider = ProviderBuilder::new().wallet(wallet).connect_http(
            rpc_url
                .parse()
                .map_err(|e| format!("Invalid RPC URL '{rpc_url}': {e}"))?,
        );

        Ok(provider)
    }
}

/// Central coordinator for wallet operations
///
/// The WalletManager provides a high-level interface for:
/// - Acquiring wallets for operations (with automatic locking)
/// - Managing the wallet pool
/// - Looking up beacon-to-wallet mappings
pub struct WalletManager {
    /// The wallet pool (None in test stub mode)
    pool: Option<WalletPool>,
    /// Configuration (None in test stub mode)
    config: Option<WalletManagerConfig>,
    /// Whether this is a test stub that will panic on use
    is_test_stub: bool,
    /// Mock signers for testing (bypasses Turnkey API)
    mock_signers: Option<HashMap<Address, PrivateKeySigner>>,
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

        Ok(Self {
            pool: Some(pool),
            config: Some(config),
            is_test_stub: false,
            mock_signers: None,
        })
    }

    /// Create a test stub WalletManager that panics when used
    ///
    /// This is for test utilities that need to construct AppState
    /// but don't actually use WalletManager features.
    pub fn test_stub() -> Self {
        Self {
            pool: None,
            config: None,
            is_test_stub: true,
            mock_signers: None,
        }
    }

    /// Create a WalletManager with mock local signers for testing
    /// This allows wallet operations to work without real Turnkey credentials
    pub async fn test_with_mock_signers(
        redis_url: &str,
        signers: Vec<PrivateKeySigner>,
    ) -> Result<Self, String> {
        Self::test_with_mock_signers_and_prefix(redis_url, signers, "beaconator:").await
    }

    /// Create a WalletManager with mock local signers and a custom prefix for testing
    ///
    /// Using a unique prefix per test allows tests to run in parallel without
    /// conflicting over shared Redis state.
    pub async fn test_with_mock_signers_and_prefix(
        redis_url: &str,
        signers: Vec<PrivateKeySigner>,
        prefix: &str,
    ) -> Result<Self, String> {
        let instance_id = format!("test-{}", uuid::Uuid::new_v4());
        let pool = WalletPool::with_prefix(redis_url, instance_id, prefix).await?;

        let mock_signers: HashMap<Address, PrivateKeySigner> =
            signers.into_iter().map(|s| (s.address(), s)).collect();

        Ok(Self {
            pool: Some(pool),
            config: None,
            is_test_stub: false,
            mock_signers: Some(mock_signers),
        })
    }

    /// Get addresses of all mock signers (for populating wallet pool in tests)
    pub fn mock_signer_addresses(&self) -> Vec<Address> {
        self.mock_signers
            .as_ref()
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default()
    }

    fn require_pool(&self) -> &WalletPool {
        self.pool.as_ref().unwrap_or_else(|| {
            panic!(
                "WalletManager::test_stub() was used but wallet operations were attempted. \
                 This test needs to be updated to use a real WalletManager with Redis."
            )
        })
    }

    fn require_config(&self) -> &WalletManagerConfig {
        self.config.as_ref().unwrap_or_else(|| {
            panic!(
                "WalletManager::test_stub() was used but wallet operations were attempted. \
                 This test needs to be updated to use a real WalletManager with Redis."
            )
        })
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
        let pool = self.require_pool();
        // Check if beacon has a designated wallet
        if let Some(wallet_address) = pool.get_wallet_for_beacon(beacon).await? {
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
        // If we have mock signers, use them directly (for testing)
        if let Some(mock_signers) = &self.mock_signers {
            let pool = self.require_pool();

            // Check if we have a mock signer for this address
            if let Some(signer) = mock_signers.get(address) {
                // Create lock for this wallet using pool's keys for prefix isolation
                let lock = WalletLock::with_keys(
                    pool.redis_client().clone(),
                    *address,
                    pool.instance_id().to_string(),
                    std::time::Duration::from_secs(30), // Default TTL for tests
                    pool.keys(),
                );

                let lock_guard = lock
                    .acquire(3, std::time::Duration::from_millis(100))
                    .await?;

                return Ok(WalletHandle {
                    signer: WalletSigner::Local(signer.clone()),
                    lock_guard,
                });
            } else {
                return Err(format!("No mock signer available for wallet {address}"));
            }
        }

        // Otherwise use Turnkey (existing production logic)
        let pool = self.require_pool();
        let config = self.require_config();

        // Get wallet info from pool
        let wallet_info = pool.get_wallet_info(address).await?;

        // Create and acquire lock using pool's keys for prefix consistency
        let lock = WalletLock::with_keys(
            pool.redis_client().clone(),
            *address,
            pool.instance_id().to_string(),
            config.lock_ttl,
            pool.keys(),
        );

        let lock_guard = lock
            .acquire(config.lock_retry_count, config.lock_retry_delay)
            .await?;

        // Create signer
        let turnkey_signer = TurnkeySigner::new(
            config.turnkey_api_url.clone(),
            config.turnkey_organization_id.clone(),
            config.turnkey_api_public_key.clone(),
            config.turnkey_api_private_key.clone(),
            wallet_info.turnkey_key_id.clone(),
            wallet_info.address,
            config.chain_id,
        )
        .map_err(|e| format!("Failed to create Turnkey signer: {e}"))?;

        Ok(WalletHandle {
            signer: WalletSigner::Turnkey(turnkey_signer),
            lock_guard,
        })
    }

    /// Acquire any available wallet from the pool
    pub async fn acquire_any_wallet(&self) -> Result<WalletHandle, String> {
        // If we have mock signers, use them directly (for testing)
        if let Some(mock_signers) = &self.mock_signers {
            let pool = self.require_pool();
            let available = pool.list_available_wallets().await?;

            if available.is_empty() {
                return Err("No available wallets in the pool".to_string());
            }

            // Find a mock signer for an available wallet
            for wallet_info in available {
                if let Some(signer) = mock_signers.get(&wallet_info.address) {
                    // Create lock for this wallet using pool's keys for prefix isolation
                    let lock = WalletLock::with_keys(
                        pool.redis_client().clone(),
                        wallet_info.address,
                        pool.instance_id().to_string(),
                        std::time::Duration::from_secs(30), // Default TTL for tests
                        pool.keys(),
                    );

                    if let Ok(lock_guard) =
                        lock.acquire(3, std::time::Duration::from_millis(100)).await
                    {
                        return Ok(WalletHandle {
                            signer: WalletSigner::Local(signer.clone()),
                            lock_guard,
                        });
                    }
                }
            }
            return Err("Failed to acquire any wallet from the pool".to_string());
        }

        // Otherwise use Turnkey (existing production logic)
        let pool = self.require_pool();
        let config = self.require_config();

        let available = pool.list_available_wallets().await?;

        if available.is_empty() {
            return Err("No available wallets in the pool".to_string());
        }

        // Try to acquire each available wallet until one succeeds
        let mut last_error: Option<String> = None;

        for wallet_info in available {
            let lock = WalletLock::with_keys(
                pool.redis_client().clone(),
                wallet_info.address,
                pool.instance_id().to_string(),
                config.lock_ttl,
                pool.keys(),
            );

            match lock
                .acquire(config.lock_retry_count, config.lock_retry_delay)
                .await
            {
                Ok(lock_guard) => {
                    match TurnkeySigner::new(
                        config.turnkey_api_url.clone(),
                        config.turnkey_organization_id.clone(),
                        config.turnkey_api_public_key.clone(),
                        config.turnkey_api_private_key.clone(),
                        wallet_info.turnkey_key_id.clone(),
                        wallet_info.address,
                        config.chain_id,
                    ) {
                        Ok(turnkey_signer) => {
                            return Ok(WalletHandle {
                                signer: WalletSigner::Turnkey(turnkey_signer),
                                lock_guard,
                            });
                        }
                        Err(e) => {
                            // Log and continue trying other wallets
                            let error_msg = format!(
                                "Failed to create Turnkey signer for wallet {}: {e}",
                                wallet_info.address
                            );
                            tracing::warn!("{}", error_msg);
                            last_error = Some(error_msg);
                            // lock_guard drops here, releasing the lock
                            continue;
                        }
                    }
                }
                Err(_) => continue, // Try next wallet
            }
        }

        Err(last_error.unwrap_or_else(|| "Failed to acquire any wallet from the pool".to_string()))
    }

    /// Get access to the wallet pool
    pub fn pool(&self) -> &WalletPool {
        self.require_pool()
    }

    /// Get the instance ID
    pub fn instance_id(&self) -> &str {
        self.require_pool().instance_id()
    }

    /// Create a lock for a specific wallet
    ///
    /// This is useful when you need to manage the lock separately from
    /// acquiring a wallet handle.
    pub fn create_lock(&self, address: &Address) -> WalletLock {
        let pool = self.require_pool();
        let config = self.require_config();
        WalletLock::with_keys(
            pool.redis_client().clone(),
            *address,
            pool.instance_id().to_string(),
            config.lock_ttl,
            pool.keys(),
        )
    }

    /// List all wallets in the pool
    pub async fn list_wallets(&self) -> Result<Vec<WalletInfo>, String> {
        self.require_pool().list_wallets().await
    }

    /// Check if this is a test stub
    pub fn is_test_stub(&self) -> bool {
        self.is_test_stub
    }
}
