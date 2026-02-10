//! Mock implementations for testing wallet services
//!
//! This module provides mock implementations of the wallet services
//! for use in unit tests without requiring Redis connections.

use alloy::network::EthereumWallet;
use alloy::primitives::Address;
use alloy::providers::ProviderBuilder;
use alloy::signers::Signer;
use alloy::signers::local::PrivateKeySigner;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::AlloyProvider;
use crate::models::wallet::{WalletInfo, WalletStatus};

/// Mock wallet pool for testing
#[derive(Debug, Clone, Default)]
pub struct MockWalletPool {
    wallets: Arc<RwLock<HashMap<Address, WalletInfo>>>,
}

impl MockWalletPool {
    /// Create a new mock wallet pool
    pub fn new() -> Self {
        Self {
            wallets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a wallet to the mock pool
    pub fn add_wallet(&self, wallet: WalletInfo) {
        let mut wallets = self.wallets.write().unwrap();
        wallets.insert(wallet.address, wallet);
    }

    /// Get a wallet from the mock pool
    pub fn get_wallet(&self, address: &Address) -> Option<WalletInfo> {
        let wallets = self.wallets.read().unwrap();
        wallets.get(address).cloned()
    }

    /// List all wallets in the mock pool
    pub fn list_wallets(&self) -> Vec<WalletInfo> {
        let wallets = self.wallets.read().unwrap();
        wallets.values().cloned().collect()
    }

    /// List available wallets in the mock pool
    pub fn list_available_wallets(&self) -> Vec<WalletInfo> {
        let wallets = self.wallets.read().unwrap();
        wallets
            .values()
            .filter(|w| matches!(w.status, WalletStatus::Available))
            .cloned()
            .collect()
    }

    /// Update a wallet's status
    pub fn update_wallet_status(&self, address: &Address, status: WalletStatus) -> bool {
        let mut wallets = self.wallets.write().unwrap();
        if let Some(wallet) = wallets.get_mut(address) {
            wallet.status = status;
            true
        } else {
            false
        }
    }
}

/// Mock beacon-to-wallet mapping for testing
#[derive(Debug, Clone, Default)]
pub struct MockBeaconMapping {
    mappings: Arc<RwLock<HashMap<Address, Address>>>,
}

impl MockBeaconMapping {
    /// Create a new mock beacon mapping
    pub fn new() -> Self {
        Self {
            mappings: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set a beacon-to-wallet mapping
    pub fn set_mapping(&self, beacon: Address, wallet: Address) {
        let mut mappings = self.mappings.write().unwrap();
        mappings.insert(beacon, wallet);
    }

    /// Get the wallet for a beacon
    pub fn get_wallet_for_beacon(&self, beacon: &Address) -> Option<Address> {
        let mappings = self.mappings.read().unwrap();
        mappings.get(beacon).copied()
    }

    /// Remove a beacon mapping
    pub fn remove_mapping(&self, beacon: &Address) -> bool {
        let mut mappings = self.mappings.write().unwrap();
        mappings.remove(beacon).is_some()
    }
}

/// Mock wallet lock for testing
#[derive(Debug, Clone, Default)]
pub struct MockWalletLock {
    locks: Arc<RwLock<HashMap<Address, String>>>,
}

impl MockWalletLock {
    /// Create a new mock wallet lock
    pub fn new() -> Self {
        Self {
            locks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Try to acquire a lock
    pub fn acquire(&self, address: &Address, instance_id: &str) -> bool {
        let mut locks = self.locks.write().unwrap();
        if locks.contains_key(address) {
            false
        } else {
            locks.insert(*address, instance_id.to_string());
            true
        }
    }

    /// Release a lock
    pub fn release(&self, address: &Address, instance_id: &str) -> bool {
        let mut locks = self.locks.write().unwrap();
        if locks.get(address) == Some(&instance_id.to_string()) {
            locks.remove(address);
            true
        } else {
            false
        }
    }

    /// Check if a wallet is locked
    pub fn is_locked(&self, address: &Address) -> bool {
        let locks = self.locks.read().unwrap();
        locks.contains_key(address)
    }

    /// Get the lock holder
    pub fn lock_holder(&self, address: &Address) -> Option<String> {
        let locks = self.locks.read().unwrap();
        locks.get(address).cloned()
    }
}

/// A mock wallet handle for testing that uses PrivateKeySigner
pub struct MockWalletHandle {
    /// The signer for this wallet
    pub signer: PrivateKeySigner,
    /// The address of this wallet
    address: Address,
}

impl MockWalletHandle {
    /// Get the Ethereum address of this wallet
    pub fn address(&self) -> Address {
        self.address
    }

    /// Build an AlloyProvider using this wallet's signer
    pub fn build_provider(&self, rpc_url: &str) -> Result<AlloyProvider, String> {
        let wallet = EthereumWallet::from(self.signer.clone());

        let provider = ProviderBuilder::new().wallet(wallet).connect_http(
            rpc_url
                .parse()
                .map_err(|e| format!("Invalid RPC URL '{rpc_url}': {e}"))?,
        );

        Ok(provider)
    }
}

/// Mock WalletManager for testing without Redis
///
/// This provides the same interface as WalletManager but uses
/// local PrivateKeySigners for simplicity in tests.
pub struct MockWalletManager {
    signers: Vec<PrivateKeySigner>,
    current_index: RwLock<usize>,
}

impl MockWalletManager {
    /// Create a new MockWalletManager with the given signers
    pub fn new(signers: Vec<PrivateKeySigner>) -> Self {
        Self {
            signers,
            current_index: RwLock::new(0),
        }
    }

    /// Create a MockWalletManager from Anvil's deterministic test keys
    pub fn from_anvil_keys() -> Self {
        // Anvil's first 3 deterministic private keys
        let keys = [
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80",
            "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
            "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
        ];

        let signers: Vec<PrivateKeySigner> = keys
            .iter()
            .map(|k| {
                k.parse::<PrivateKeySigner>()
                    .expect("Failed to parse test private key")
                    .with_chain_id(Some(31337))
            })
            .collect();

        Self::new(signers)
    }

    /// Acquire a wallet for a beacon (returns mock handle)
    pub async fn acquire_for_beacon(&self, _beacon: &Address) -> Result<MockWalletHandle, String> {
        self.acquire_any_wallet().await
    }

    /// Acquire any available wallet from the pool
    pub async fn acquire_any_wallet(&self) -> Result<MockWalletHandle, String> {
        if self.signers.is_empty() {
            return Err("No signers available in mock wallet manager".to_string());
        }

        let mut index = self.current_index.write().unwrap();
        let signer = self.signers[*index % self.signers.len()].clone();
        *index = (*index + 1) % self.signers.len();

        Ok(MockWalletHandle {
            address: signer.address(),
            signer,
        })
    }

    /// Get the address of the first signer (for backwards compatibility in tests)
    pub fn first_address(&self) -> Option<Address> {
        self.signers.first().map(|s| s.address())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::address;

    #[test]
    fn test_mock_wallet_pool() {
        let pool = MockWalletPool::new();
        let addr = address!("0x1234567890123456789012345678901234567890");

        let wallet = WalletInfo {
            address: addr,
            key_id: "test-key".to_string(),
            status: WalletStatus::Available,
            designated_beacons: vec![],
        };

        pool.add_wallet(wallet.clone());

        assert!(pool.get_wallet(&addr).is_some());
        assert_eq!(pool.list_wallets().len(), 1);
        assert_eq!(pool.list_available_wallets().len(), 1);

        pool.update_wallet_status(
            &addr,
            WalletStatus::Locked {
                by_instance: "test".to_string(),
                since_timestamp: 0,
            },
        );

        assert_eq!(pool.list_available_wallets().len(), 0);
    }

    #[test]
    fn test_mock_beacon_mapping() {
        let mapping = MockBeaconMapping::new();
        let beacon = address!("0x1111111111111111111111111111111111111111");
        let wallet = address!("0x2222222222222222222222222222222222222222");

        assert!(mapping.get_wallet_for_beacon(&beacon).is_none());

        mapping.set_mapping(beacon, wallet);

        assert_eq!(mapping.get_wallet_for_beacon(&beacon), Some(wallet));

        assert!(mapping.remove_mapping(&beacon));
        assert!(mapping.get_wallet_for_beacon(&beacon).is_none());
    }

    #[test]
    fn test_mock_wallet_lock() {
        let lock = MockWalletLock::new();
        let addr = address!("0x1234567890123456789012345678901234567890");

        assert!(!lock.is_locked(&addr));
        assert!(lock.acquire(&addr, "instance-1"));
        assert!(lock.is_locked(&addr));
        assert_eq!(lock.lock_holder(&addr), Some("instance-1".to_string()));

        // Can't acquire again
        assert!(!lock.acquire(&addr, "instance-2"));

        // Wrong instance can't release
        assert!(!lock.release(&addr, "instance-2"));

        // Correct instance can release
        assert!(lock.release(&addr, "instance-1"));
        assert!(!lock.is_locked(&addr));
    }
}
