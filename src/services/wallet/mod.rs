//! Multi-wallet management service using Turnkey
//!
//! This module provides distributed wallet management for multiple beaconator instances:
//! - TurnkeySigner: Alloy Signer implementation backed by Turnkey API
//! - WalletPool: Redis-backed pool of available wallets (includes beacon→wallet mappings)
//! - WalletLock: Distributed locking to prevent concurrent wallet use
//! - WalletManager: Central coordinator for wallet operations

pub mod lock;
pub mod manager;
pub mod mock;
pub mod pool;
pub mod sync;
pub mod turnkey_api;
pub mod turnkey_signer;

pub use lock::{WalletLock, WalletLockGuard};
pub use manager::{WalletHandle, WalletManager, WalletSigner};
pub use mock::{MockWalletHandle, MockWalletManager};
pub use pool::WalletPool;
pub use sync::{SyncResult, WalletSyncService};
pub use turnkey_api::{TurnkeyWalletAPI, TurnkeyWalletAPIError, TurnkeyWalletAccount};
pub use turnkey_signer::TurnkeySigner;

// Re-export model types for convenience
pub use crate::models::wallet::{RedisKeys, WalletInfo, WalletManagerConfig, WalletStatus};
