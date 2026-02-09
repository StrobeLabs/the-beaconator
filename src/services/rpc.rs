use alloy::network::EthereumWallet;
use alloy::primitives::Address;
use alloy::providers::ProviderBuilder;
use alloy::signers::{Signer, local::PrivateKeySigner};
use std::env;

// Import provider types from lib.rs
use crate::{AlloyProvider, ReadOnlyProvider};

/// Configuration for RPC endpoints
#[derive(Debug, Clone)]
pub struct RpcConfig {
    pub env_type: String,
    pub rpc_url: String,
}

impl RpcConfig {
    /// Load RPC configuration from environment variables
    pub fn from_env() -> Result<Self, String> {
        let env_type = env::var("ENV").map_err(|_| {
            "ENV environment variable not set. Must be 'mainnet', 'testnet', or 'localnet'"
                .to_string()
        })?;

        // Validate ENV value
        match env_type.to_lowercase().as_str() {
            "mainnet" | "testnet" | "localnet" => {}
            _ => {
                return Err(format!(
                    "Invalid ENV value '{env_type}'. Must be 'mainnet', 'testnet', or 'localnet'"
                ));
            }
        }

        let rpc_url = env::var("RPC_URL").map_err(|_| {
            "RPC_URL environment variable not set. Must be a complete RPC URL with API key."
                .to_string()
        })?;

        tracing::info!("Using RPC endpoint from RPC_URL");

        Ok(Self { env_type, rpc_url })
    }

    /// Helper function to build a provider from a URL and private key
    fn build_provider_from_url(
        private_key: &str,
        chain_id: u64,
        url: &str,
    ) -> Result<AlloyProvider, String> {
        let signer = private_key
            .parse::<PrivateKeySigner>()
            .map_err(|e| format!("Failed to parse private key: {e}"))?
            .with_chain_id(Some(chain_id));

        let wallet = EthereumWallet::from(signer);

        let provider = ProviderBuilder::new().wallet(wallet).connect_http(
            url.parse()
                .map_err(|e| format!("Invalid RPC URL '{url}': {e}"))?,
        );

        Ok(provider)
    }

    /// Build a read-only provider from a URL (no wallet, for queries only)
    pub fn build_read_only_provider(url: &str) -> Result<ReadOnlyProvider, String> {
        let provider = ProviderBuilder::new().connect_http(
            url.parse()
                .map_err(|e| format!("Invalid RPC URL '{url}': {e}"))?,
        );

        Ok(provider)
    }

    /// Build a read-only RPC provider (no wallet, for queries only)
    pub fn build_read_only_provider_from_config(&self) -> Result<ReadOnlyProvider, String> {
        let provider = Self::build_read_only_provider(&self.rpc_url)?;
        tracing::info!("Read-only RPC provider setup successful");
        Ok(provider)
    }

    /// Get the RPC URL (public accessor for use by WalletHandle)
    pub fn rpc_url(&self) -> &str {
        &self.rpc_url
    }

    /// Build an RPC provider with the given wallet and chain ID
    pub fn build_provider(
        &self,
        private_key: &str,
        chain_id: u64,
    ) -> Result<AlloyProvider, String> {
        let provider = Self::build_provider_from_url(private_key, chain_id, &self.rpc_url)?;
        tracing::info!("RPC provider setup successful");
        Ok(provider)
    }

    /// Get the wallet address from a private key
    pub fn get_wallet_address(private_key: &str) -> Result<Address, String> {
        let signer = private_key
            .parse::<PrivateKeySigner>()
            .map_err(|e| format!("Failed to parse private key: {e}"))?;
        Ok(signer.address())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// Anvil's first test account private key (well-known, deterministic)
    /// This is a standard test key used across Ethereum tooling, not a secret.
    /// Address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
    const ANVIL_TEST_PRIVATE_KEY: &str =
        "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    const ANVIL_TEST_ADDRESS: &str = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266";

    // Test helper to create a config directly (bypassing env vars)
    fn create_test_config(env_type: &str, rpc_url: &str) -> RpcConfig {
        RpcConfig {
            env_type: env_type.to_string(),
            rpc_url: rpc_url.to_string(),
        }
    }

    #[test]
    fn test_rpc_config_stores_url() {
        let config = create_test_config("mainnet", "https://example.com/api-key");
        assert_eq!(config.rpc_url(), "https://example.com/api-key");
        assert_eq!(config.env_type, "mainnet");
    }

    #[test]
    fn test_rpc_config_stores_env_type() {
        let config = create_test_config("testnet", "https://example.com");
        assert_eq!(config.env_type, "testnet");

        let config = create_test_config("localnet", "http://localhost:8545");
        assert_eq!(config.env_type, "localnet");
    }

    #[test]
    #[serial]
    fn test_from_env_success() {
        unsafe {
            std::env::set_var("ENV", "mainnet");
            std::env::set_var("RPC_URL", "https://rpc.example.com/key123");
        }

        let config = RpcConfig::from_env().unwrap();
        assert_eq!(config.env_type, "mainnet");
        assert_eq!(config.rpc_url(), "https://rpc.example.com/key123");

        unsafe {
            std::env::remove_var("ENV");
            std::env::remove_var("RPC_URL");
        }
    }

    #[test]
    #[serial]
    fn test_from_env_valid_env_types() {
        for env_val in &["mainnet", "testnet", "localnet", "MAINNET", "TestNet"] {
            unsafe {
                std::env::set_var("ENV", env_val);
                std::env::set_var("RPC_URL", "https://example.com");
            }
            let result = RpcConfig::from_env();
            assert!(result.is_ok(), "Should accept ENV={env_val}");
            unsafe {
                std::env::remove_var("ENV");
                std::env::remove_var("RPC_URL");
            }
        }
    }

    #[test]
    #[serial]
    fn test_from_env_invalid_env_type() {
        unsafe {
            std::env::set_var("ENV", "invalid");
            std::env::set_var("RPC_URL", "https://example.com");
        }
        let result = RpcConfig::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid ENV value 'invalid'"));
        unsafe {
            std::env::remove_var("ENV");
            std::env::remove_var("RPC_URL");
        }
    }

    #[test]
    #[serial]
    fn test_from_env_missing_env() {
        unsafe {
            std::env::remove_var("ENV");
            std::env::set_var("RPC_URL", "https://example.com");
        }
        let result = RpcConfig::from_env();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("ENV environment variable not set")
        );
        unsafe {
            std::env::remove_var("RPC_URL");
        }
    }

    #[test]
    #[serial]
    fn test_from_env_missing_rpc_url() {
        unsafe {
            std::env::set_var("ENV", "mainnet");
            std::env::remove_var("RPC_URL");
        }
        let result = RpcConfig::from_env();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("RPC_URL environment variable not set")
        );
        unsafe {
            std::env::remove_var("ENV");
        }
    }

    #[test]
    fn test_build_read_only_provider_valid_url() {
        let result = RpcConfig::build_read_only_provider("http://localhost:8545");
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_read_only_provider_invalid_url() {
        let result = RpcConfig::build_read_only_provider("not-a-valid-url");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid RPC URL"));
    }

    #[test]
    fn test_build_read_only_provider_from_config() {
        let config = create_test_config("mainnet", "http://localhost:8545");
        let result = config.build_read_only_provider_from_config();
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_provider_valid() {
        let config = create_test_config("mainnet", "http://localhost:8545");
        let result = config.build_provider(ANVIL_TEST_PRIVATE_KEY, 8453);
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_provider_invalid_key() {
        let config = create_test_config("mainnet", "http://localhost:8545");
        let result = config.build_provider("invalid-key", 8453);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse private key"));
    }

    #[test]
    fn test_get_wallet_address_valid() {
        let result = RpcConfig::get_wallet_address(ANVIL_TEST_PRIVATE_KEY);
        assert!(result.is_ok());
        // This key corresponds to Anvil's first test account
        assert_eq!(
            result.unwrap().to_string().to_lowercase(),
            ANVIL_TEST_ADDRESS
        );
    }

    #[test]
    fn test_get_wallet_address_invalid() {
        let result = RpcConfig::get_wallet_address("invalid");
        assert!(result.is_err());
    }
}
