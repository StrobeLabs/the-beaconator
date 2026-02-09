use alloy::primitives::Address;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Status of a wallet in the pool
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum WalletStatus {
    /// Wallet is available for use
    Available,
    /// Wallet is currently locked by an instance
    Locked {
        by_instance: String,
        since_timestamp: u64,
    },
    /// Wallet is reserved for specific beacons
    Reserved {
        #[schemars(with = "Vec<String>")]
        for_beacons: Vec<Address>,
    },
}

/// Information about a wallet in the pool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WalletInfo {
    /// Ethereum address of the wallet
    #[schemars(with = "String")]
    pub address: Address,
    /// Turnkey wallet ID or private key ID
    pub turnkey_key_id: String,
    /// Current status of the wallet
    pub status: WalletStatus,
    /// Beacons that require this specific wallet as their ECDSA signer
    #[schemars(with = "Vec<String>")]
    pub designated_beacons: Vec<Address>,
}

/// Configuration for the wallet manager
#[derive(Debug, Clone)]
pub struct WalletManagerConfig {
    /// Redis connection URL
    pub redis_url: String,
    /// Turnkey API base URL
    pub turnkey_api_url: String,
    /// Turnkey organization ID
    pub turnkey_organization_id: String,
    /// Turnkey API public key
    pub turnkey_api_public_key: String,
    /// Turnkey API private key
    pub turnkey_api_private_key: String,
    /// Lock TTL - how long a wallet lock is held before expiring
    pub lock_ttl: Duration,
    /// Number of retries when acquiring a lock
    pub lock_retry_count: u32,
    /// Delay between lock acquisition retries
    pub lock_retry_delay: Duration,
    /// Optional instance ID - if not provided, a UUID will be generated
    pub instance_id: Option<String>,
    /// Chain ID for EIP-155 signatures (e.g., 8453 for Base mainnet)
    pub chain_id: Option<u64>,
    /// Allowed Turnkey wallet IDs - only wallets with these IDs will be synced
    /// If empty, all wallets in the organization will be synced (not recommended for production)
    pub allowed_wallet_ids: Vec<String>,
}

impl WalletManagerConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self, String> {
        let chain_id = std::env::var("CHAIN_ID")
            .ok()
            .and_then(|s| s.parse::<u64>().ok());

        // Parse allowed wallet IDs from comma-separated list
        let allowed_wallet_ids = std::env::var("BEACONATOR_WALLET_IDS")
            .map(|s| {
                s.split(',')
                    .map(|id| id.trim().to_string())
                    .filter(|id| !id.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string()),
            turnkey_api_url: std::env::var("TURNKEY_API_URL")
                .unwrap_or_else(|_| "https://api.turnkey.com".to_string()),
            turnkey_organization_id: std::env::var("TURNKEY_ORGANIZATION_ID")
                .map_err(|_| "TURNKEY_ORGANIZATION_ID environment variable not set")?,
            turnkey_api_public_key: std::env::var("TURNKEY_API_PUBLIC_KEY")
                .map_err(|_| "TURNKEY_API_PUBLIC_KEY environment variable not set")?,
            turnkey_api_private_key: std::env::var("TURNKEY_API_PRIVATE_KEY")
                .map_err(|_| "TURNKEY_API_PRIVATE_KEY environment variable not set")?,
            lock_ttl: Duration::from_secs(60),
            lock_retry_count: 10,
            lock_retry_delay: Duration::from_millis(500),
            instance_id: std::env::var("BEACONATOR_INSTANCE_ID").ok(),
            chain_id,
            allowed_wallet_ids,
        })
    }
}

/// Redis key generator with configurable prefix for test isolation
///
/// Each WalletPool instance can have its own prefix, allowing tests to run
/// in parallel without conflicting over shared Redis keys.
#[derive(Debug, Clone)]
pub struct PrefixedRedisKeys {
    prefix: String,
}

impl PrefixedRedisKeys {
    /// Create a new key generator with the given prefix
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
        }
    }

    /// Get the prefix used by this key generator
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Set of all wallet addresses in the pool
    pub fn wallet_pool(&self) -> String {
        format!("{}wallet_pool", self.prefix)
    }

    /// Hash storing wallet metadata: wallet:{address} -> WalletInfo JSON
    pub fn wallet_info(&self, address: &Address) -> String {
        format!("{}wallet:{address}", self.prefix)
    }

    /// Lock key for a specific wallet: wallet_lock:{address}
    pub fn wallet_lock(&self, address: &Address) -> String {
        format!("{}wallet_lock:{address}", self.prefix)
    }

    /// Mapping from beacon address to designated wallet: beacon_wallet:{beacon}
    pub fn beacon_wallet(&self, beacon: &Address) -> String {
        format!("{}beacon_wallet:{beacon}", self.prefix)
    }

    /// Reverse mapping: which beacons use a wallet: wallet_beacons:{wallet}
    pub fn wallet_beacons(&self, wallet: &Address) -> String {
        format!("{}wallet_beacons:{wallet}", self.prefix)
    }
}

impl Default for PrefixedRedisKeys {
    fn default() -> Self {
        Self::new("beaconator:")
    }
}

/// Redis key patterns for wallet management (deprecated - use PrefixedRedisKeys)
///
/// This struct provides static methods for backwards compatibility.
/// New code should use PrefixedRedisKeys for test isolation.
pub struct RedisKeys;

impl RedisKeys {
    /// Set of all wallet addresses in the pool
    pub fn wallet_pool() -> &'static str {
        "beaconator:wallet_pool"
    }

    /// Hash storing wallet metadata: wallet:{address} -> WalletInfo JSON
    pub fn wallet_info(address: &Address) -> String {
        format!("beaconator:wallet:{address}")
    }

    /// Lock key for a specific wallet: wallet_lock:{address}
    pub fn wallet_lock(address: &Address) -> String {
        format!("beaconator:wallet_lock:{address}")
    }

    /// Mapping from beacon address to designated wallet: beacon_wallet:{beacon}
    pub fn beacon_wallet(beacon: &Address) -> String {
        format!("beaconator:beacon_wallet:{beacon}")
    }

    /// Reverse mapping: which beacons use a wallet: wallet_beacons:{wallet}
    pub fn wallet_beacons(wallet: &Address) -> String {
        format!("beaconator:wallet_beacons:{wallet}")
    }
}
