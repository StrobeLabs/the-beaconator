//! Central wallet management coordinator
//!
//! This module provides the WalletManager, which coordinates wallet pool
//! operations, locking, and beacon mappings into a unified interface.

use std::collections::HashMap;
use std::time::Duration;

use super::lock::LockHeartbeat;
use super::{WalletLock, WalletLockGuard, WalletPool};
use alloy::network::EthereumWallet;
use alloy::primitives::{Address, B256};
use alloy::providers::ProviderBuilder;
use alloy::signers::aws::AwsSigner;
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::{Error as SignerError, Signature, Signer};

use crate::AlloyProvider;
use crate::models::wallet::{WalletInfo, WalletManagerConfig};

/// A gas-payer pool signer: either a local private key (dev/CI) or an AWS KMS
/// key (production). The pool is keyed by Ethereum address regardless of backend.
#[derive(Clone)]
pub enum PoolSigner {
    /// Local secp256k1 private key held in memory.
    Local(PrivateKeySigner),
    /// AWS KMS `ECC_SECG_P256K1` key; the private key never leaves KMS.
    Kms(AwsSigner),
}

impl PoolSigner {
    /// The Ethereum address of this signer (cached at construction for KMS).
    pub fn address(&self) -> Address {
        match self {
            PoolSigner::Local(s) => s.address(),
            PoolSigner::Kms(s) => s.address(),
        }
    }

    /// Sign a 32-byte hash with the underlying backend.
    pub async fn sign_hash(&self, hash: &B256) -> Result<Signature, SignerError> {
        match self {
            PoolSigner::Local(s) => s.sign_hash(hash).await,
            PoolSigner::Kms(s) => s.sign_hash(hash).await,
        }
    }

    /// Wrap this signer into an `EthereumWallet` for transaction sending.
    fn ethereum_wallet(&self) -> EthereumWallet {
        match self {
            PoolSigner::Local(s) => EthereumWallet::from(s.clone()),
            PoolSigner::Kms(s) => EthereumWallet::from(s.clone()),
        }
    }
}

/// Pool signer wrapper (local key or KMS).
#[derive(Clone)]
pub struct WalletSigner(PoolSigner);

impl WalletSigner {
    /// Get the address of the signer
    pub fn address(&self) -> Address {
        self.0.address()
    }

    /// Sign a hash using the underlying signer
    pub async fn sign_hash(&self, hash: &B256) -> Result<Signature, SignerError> {
        self.0.sign_hash(hash).await
    }
}

/// A handle to a locked wallet ready for use
///
/// This combines the signer with its lock guard, ensuring the wallet
/// remains locked for the duration of operations. A background heartbeat
/// keeps the Redis lock alive for as long as the handle lives — flows like
/// USDC approval waits and modular recipes hold a wallet far longer than
/// the lock TTL.
///
/// Field order matters: `heartbeat` is declared before `lock_guard` so it is
/// dropped (and its extension task aborted) BEFORE the guard releases the lock.
pub struct WalletHandle {
    /// The signer for this wallet
    pub signer: WalletSigner,
    /// Background lock-extension task; aborted on drop, before the lock release
    heartbeat: LockHeartbeat,
    /// The lock guard - wallet is locked until this is dropped
    pub lock_guard: WalletLockGuard,
}

impl WalletHandle {
    /// Create a handle and start its lock heartbeat (extends every `lock_ttl / 3`)
    fn new(signer: WalletSigner, lock_guard: WalletLockGuard, lock_ttl: Duration) -> Self {
        let heartbeat = lock_guard.spawn_heartbeat(lock_ttl);
        Self {
            signer,
            heartbeat,
            lock_guard,
        }
    }

    /// Get the Ethereum address of this wallet
    pub fn address(&self) -> Address {
        self.signer.address()
    }

    /// Error if the distributed lock backing this handle has been lost.
    ///
    /// Call this immediately before every transaction send: a lost lock means
    /// another instance may already be using this wallet, and sending would
    /// risk a nonce collision.
    pub fn ensure_lock_held(&self) -> Result<(), String> {
        self.heartbeat.ensure_held()
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
        let wallet = self.signer.0.ethereum_wallet();

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
    /// Pool signers (local key or KMS) keyed by address
    signers: HashMap<Address, PoolSigner>,
}

impl WalletManager {
    /// Create a new WalletManager with local private key signers
    ///
    /// # Arguments
    /// * `config` - Configuration for the wallet manager
    /// * `signers` - Pool signers (local key or KMS) for the wallet pool
    pub async fn new(
        config: WalletManagerConfig,
        signers: Vec<PoolSigner>,
    ) -> Result<Self, String> {
        let instance_id = config
            .instance_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let pool = WalletPool::new(&config.redis_url, instance_id).await?;

        let signers_map: HashMap<Address, PoolSigner> =
            signers.into_iter().map(|s| (s.address(), s)).collect();

        Ok(Self {
            pool: Some(pool),
            config: Some(config),
            is_test_stub: false,
            signers: signers_map,
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
            signers: HashMap::new(),
        }
    }

    /// Create a WalletManager with local signers for testing
    pub async fn test_with_mock_signers(
        redis_url: &str,
        signers: Vec<PrivateKeySigner>,
    ) -> Result<Self, String> {
        Self::test_with_mock_signers_and_prefix(redis_url, signers, "beaconator:").await
    }

    /// Create a WalletManager with local signers and a custom prefix for testing
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

        let signers_map: HashMap<Address, PoolSigner> = signers
            .into_iter()
            .map(|s| (s.address(), PoolSigner::Local(s)))
            .collect();

        Ok(Self {
            pool: Some(pool),
            config: Some(WalletManagerConfig {
                redis_url: redis_url.to_string(),
                lock_ttl: std::time::Duration::from_secs(30),
                lock_retry_count: 3,
                lock_retry_delay: std::time::Duration::from_millis(100),
                instance_id: None,
                chain_id: None,
            }),
            is_test_stub: false,
            signers: signers_map,
        })
    }

    /// Get addresses of all signers (for populating wallet pool)
    pub fn signer_addresses(&self) -> Vec<Address> {
        self.signers.keys().copied().collect()
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
        let pool = self.require_pool();
        let config = self.require_config();

        let signer = self
            .signers
            .get(address)
            .ok_or_else(|| format!("No signer available for wallet {address}"))?;

        let lock = WalletLock::with_keys(
            pool.connection().clone(),
            *address,
            pool.instance_id().to_string(),
            config.lock_ttl,
            pool.keys(),
        );

        let lock_guard = lock
            .acquire(config.lock_retry_count, config.lock_retry_delay)
            .await?;

        Ok(WalletHandle::new(
            WalletSigner(signer.clone()),
            lock_guard,
            config.lock_ttl,
        ))
    }

    /// Serialize ECDSA updates for one beacon across all beaconator instances.
    ///
    /// The verifier nonce is per-beacon: two in-flight updates for the same
    /// beacon can land out of nonce order and the loser reverts on-chain. Hold
    /// the returned guard from nonce generation through receipt so updates for
    /// a beacon are strictly ordered. Field order in the tuple mirrors
    /// `WalletHandle`: the heartbeat is dropped before the guard releases.
    pub async fn acquire_beacon_update_lock(
        &self,
        beacon: Address,
    ) -> Result<(LockHeartbeat, WalletLockGuard), String> {
        let pool = self.require_pool();
        let config = self.require_config();

        let lock = WalletLock::for_beacon_update(
            pool.connection().clone(),
            beacon,
            pool.instance_id().to_string(),
            config.lock_ttl,
            pool.keys(),
        );

        let guard = lock
            .acquire(config.lock_retry_count, config.lock_retry_delay)
            .await
            .map_err(|e| format!("Failed to acquire beacon update lock for {beacon}: {e}"))?;
        let heartbeat = guard.spawn_heartbeat(config.lock_ttl);
        Ok((heartbeat, guard))
    }

    /// Acquire any available wallet from the pool
    ///
    /// First pass tries every wallet once without retrying (so one busy wallet
    /// can't head-of-line block the rest of the pool); only if ALL wallets are
    /// locked does it fall back to the retry loop.
    pub async fn acquire_any_wallet(&self) -> Result<WalletHandle, String> {
        let pool = self.require_pool();
        let config = self.require_config();

        let available = pool.list_available_wallets().await?;

        if available.is_empty() {
            return Err("No available wallets in the pool".to_string());
        }

        // Fast pass: one non-blocking attempt per wallet.
        for wallet_info in &available {
            if let Some(signer) = self.signers.get(&wallet_info.address) {
                let lock = WalletLock::with_keys(
                    pool.connection().clone(),
                    wallet_info.address,
                    pool.instance_id().to_string(),
                    config.lock_ttl,
                    pool.keys(),
                );

                if let Ok(lock_guard) = lock.try_acquire().await {
                    return Ok(WalletHandle::new(
                        WalletSigner(signer.clone()),
                        lock_guard,
                        config.lock_ttl,
                    ));
                }
            }
        }

        // Slow pass: everything was locked — wait with retries per wallet.
        for wallet_info in &available {
            if let Some(signer) = self.signers.get(&wallet_info.address) {
                let lock = WalletLock::with_keys(
                    pool.connection().clone(),
                    wallet_info.address,
                    pool.instance_id().to_string(),
                    config.lock_ttl,
                    pool.keys(),
                );

                if let Ok(lock_guard) = lock
                    .acquire(config.lock_retry_count, config.lock_retry_delay)
                    .await
                {
                    return Ok(WalletHandle::new(
                        WalletSigner(signer.clone()),
                        lock_guard,
                        config.lock_ttl,
                    ));
                }
            }
        }

        Err("Failed to acquire any wallet from the pool".to_string())
    }

    /// Get access to the wallet pool
    pub fn pool(&self) -> &WalletPool {
        self.require_pool()
    }

    /// Get the instance ID
    pub fn instance_id(&self) -> &str {
        self.require_pool().instance_id()
    }

    /// Acquire a distributed lock for a specific wallet address
    ///
    /// Unlike `acquire_specific_wallet`, this does not require the address
    /// to be in the signers map. Useful for locking wallets that are managed
    /// outside the pool (e.g., the funding wallet).
    pub async fn acquire_lock(&self, address: &Address) -> Result<WalletLockGuard, String> {
        let lock = self.create_lock(address);
        let config = self.require_config();
        lock.acquire(config.lock_retry_count, config.lock_retry_delay)
            .await
    }

    /// Create a lock for a specific wallet
    ///
    /// This is useful when you need to manage the lock separately from
    /// acquiring a wallet handle.
    pub fn create_lock(&self, address: &Address) -> WalletLock {
        let pool = self.require_pool();
        let config = self.require_config();
        WalletLock::with_keys(
            pool.connection().clone(),
            *address,
            pool.instance_id().to_string(),
            config.lock_ttl,
            pool.keys(),
        )
    }

    /// The configured lock TTL (used to size lock heartbeats)
    pub fn lock_ttl(&self) -> Duration {
        self.require_config().lock_ttl
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
