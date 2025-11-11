use alloy::network::EthereumWallet;
use alloy::primitives::Address;
use alloy::providers::ProviderBuilder;
use alloy::signers::{Signer, local::PrivateKeySigner};
use std::env;
use std::sync::Arc;

// Import the AlloyProvider type from lib.rs
use crate::AlloyProvider;

/// Configuration for RPC endpoints
#[derive(Debug, Clone)]
pub struct RpcConfig {
    pub env_type: String,
    pub rpc_url: Option<String>,
    pub rpc_api_key: Option<String>,
    pub alternate_rpc_url: Option<String>,
}

/// Container for primary and optional alternate RPC providers
pub struct RpcProviders {
    pub primary: Arc<AlloyProvider>,
    pub alternate: Option<Arc<AlloyProvider>>,
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

        let rpc_url = env::var("RPC_URL").ok();
        let rpc_api_key = env::var("RPC_API_KEY").ok().filter(|k| !k.is_empty());
        let alternate_rpc_url = env::var("BEACONATOR_ALTERNATE_RPC")
            .ok()
            .filter(|u| !u.is_empty());

        Ok(Self {
            env_type,
            rpc_url,
            rpc_api_key,
            alternate_rpc_url,
        })
    }

    /// Get the default RPC URL based on environment type
    fn get_default_rpc_url(env_type: &str) -> &'static str {
        match env_type.to_lowercase().as_str() {
            "mainnet" => "https://mainnet.base.org",
            "testnet" => "https://sepolia.base.org",
            "localnet" => "http://127.0.0.1:8545",
            _ => "https://mainnet.base.org", // Fallback (should never reach here after validation)
        }
    }

    /// Build the final RPC URL (base URL + optional API key)
    fn build_rpc_url(&self) -> String {
        let base_url = self
            .rpc_url
            .as_deref()
            .unwrap_or_else(|| Self::get_default_rpc_url(&self.env_type));

        if let Some(api_key) = &self.rpc_api_key {
            let url_with_key = format!("{}/{}", base_url.trim_end_matches('/'), api_key);
            tracing::info!("Using private RPC endpoint with API key");
            tracing::info!("Alternate RPC disabled (private primary RPC has higher reliability)");
            url_with_key
        } else {
            if self.rpc_url.is_none() {
                tracing::info!(
                    "RPC_URL not set, using default for ENV={}: {}",
                    self.env_type,
                    base_url
                );
            } else {
                tracing::info!("Using public RPC endpoint: {}", base_url);
            }
            base_url.to_string()
        }
    }

    /// Determine if alternate RPC should be used
    fn should_use_alternate(&self) -> bool {
        // Disable alternate RPC if using private primary (API key is set)
        if self.rpc_api_key.is_some() {
            return false;
        }

        // Use alternate if it's configured
        self.alternate_rpc_url.is_some()
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

    /// Build RPC providers with the given wallet and chain ID
    pub fn build_providers(
        &self,
        private_key: &str,
        chain_id: u64,
    ) -> Result<RpcProviders, String> {
        // Build primary RPC URL
        let primary_rpc_url = self.build_rpc_url();

        // Create primary provider
        let primary_provider =
            Self::build_provider_from_url(private_key, chain_id, &primary_rpc_url)?;
        tracing::info!("Primary RPC provider setup successful");

        // Build alternate provider if needed
        let alternate_provider = if self.should_use_alternate() {
            let alternate_url = self.alternate_rpc_url.as_ref().unwrap();
            tracing::info!("Setting up alternate RPC provider: {}", alternate_url);

            let provider = Self::build_provider_from_url(private_key, chain_id, alternate_url)?;
            tracing::info!("Alternate RPC provider setup successful");
            Some(Arc::new(provider))
        } else {
            if self.rpc_api_key.is_none() {
                tracing::info!(
                    "No alternate RPC configured (BEACONATOR_ALTERNATE_RPC not set or empty)"
                );
            }
            None
        };

        Ok(RpcProviders {
            primary: Arc::new(primary_provider),
            alternate: alternate_provider,
        })
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

    #[test]
    fn test_get_default_rpc_url_mainnet() {
        assert_eq!(
            RpcConfig::get_default_rpc_url("mainnet"),
            "https://mainnet.base.org"
        );
    }

    #[test]
    fn test_get_default_rpc_url_testnet() {
        assert_eq!(
            RpcConfig::get_default_rpc_url("testnet"),
            "https://sepolia.base.org"
        );
    }

    #[test]
    fn test_get_default_rpc_url_localnet() {
        assert_eq!(
            RpcConfig::get_default_rpc_url("localnet"),
            "http://127.0.0.1:8545"
        );
    }

    #[test]
    fn test_get_default_rpc_url_case_insensitive() {
        assert_eq!(
            RpcConfig::get_default_rpc_url("MAINNET"),
            "https://mainnet.base.org"
        );
        assert_eq!(
            RpcConfig::get_default_rpc_url("TestNet"),
            "https://sepolia.base.org"
        );
    }

    #[test]
    fn test_should_use_alternate_with_api_key() {
        let config = RpcConfig {
            env_type: "mainnet".to_string(),
            rpc_url: Some("https://example.com".to_string()),
            rpc_api_key: Some("test_key".to_string()),
            alternate_rpc_url: Some("https://alternate.com".to_string()),
        };

        // Alternate should be disabled when API key is present
        assert!(!config.should_use_alternate());
    }

    #[test]
    fn test_should_use_alternate_without_api_key() {
        let config = RpcConfig {
            env_type: "mainnet".to_string(),
            rpc_url: Some("https://example.com".to_string()),
            rpc_api_key: None,
            alternate_rpc_url: Some("https://alternate.com".to_string()),
        };

        // Alternate should be enabled when no API key
        assert!(config.should_use_alternate());
    }

    #[test]
    fn test_should_use_alternate_no_alternate_configured() {
        let config = RpcConfig {
            env_type: "mainnet".to_string(),
            rpc_url: Some("https://example.com".to_string()),
            rpc_api_key: None,
            alternate_rpc_url: None,
        };

        // Alternate should be disabled when not configured
        assert!(!config.should_use_alternate());
    }

    #[test]
    fn test_build_rpc_url_with_api_key() {
        let config = RpcConfig {
            env_type: "mainnet".to_string(),
            rpc_url: Some("https://base-mainnet.g.alchemy.com/v2".to_string()),
            rpc_api_key: Some("abc123xyz".to_string()),
            alternate_rpc_url: None,
        };

        let url = config.build_rpc_url();
        assert_eq!(url, "https://base-mainnet.g.alchemy.com/v2/abc123xyz");
    }

    #[test]
    fn test_build_rpc_url_with_trailing_slash() {
        let config = RpcConfig {
            env_type: "mainnet".to_string(),
            rpc_url: Some("https://base-mainnet.g.alchemy.com/v2/".to_string()),
            rpc_api_key: Some("abc123xyz".to_string()),
            alternate_rpc_url: None,
        };

        let url = config.build_rpc_url();
        assert_eq!(url, "https://base-mainnet.g.alchemy.com/v2/abc123xyz");
    }

    #[test]
    fn test_build_rpc_url_without_api_key() {
        let config = RpcConfig {
            env_type: "mainnet".to_string(),
            rpc_url: Some("https://mainnet.base.org".to_string()),
            rpc_api_key: None,
            alternate_rpc_url: None,
        };

        let url = config.build_rpc_url();
        assert_eq!(url, "https://mainnet.base.org");
    }

    #[test]
    fn test_build_rpc_url_default_mainnet() {
        let config = RpcConfig {
            env_type: "mainnet".to_string(),
            rpc_url: None,
            rpc_api_key: None,
            alternate_rpc_url: None,
        };

        let url = config.build_rpc_url();
        assert_eq!(url, "https://mainnet.base.org");
    }

    #[test]
    fn test_build_rpc_url_default_testnet() {
        let config = RpcConfig {
            env_type: "testnet".to_string(),
            rpc_url: None,
            rpc_api_key: None,
            alternate_rpc_url: None,
        };

        let url = config.build_rpc_url();
        assert_eq!(url, "https://sepolia.base.org");
    }

    #[test]
    fn test_build_rpc_url_default_localnet() {
        let config = RpcConfig {
            env_type: "localnet".to_string(),
            rpc_url: None,
            rpc_api_key: None,
            alternate_rpc_url: None,
        };

        let url = config.build_rpc_url();
        assert_eq!(url, "http://127.0.0.1:8545");
    }

    #[test]
    #[serial]
    fn test_rpc_config_validation_valid_envs() {
        // Test valid ENV values
        for env_val in &["mainnet", "testnet", "localnet"] {
            unsafe {
                std::env::set_var("ENV", env_val);
            }
            let result = RpcConfig::from_env();
            assert!(result.is_ok(), "Should accept ENV={env_val}");
            unsafe {
                std::env::remove_var("ENV");
            }
        }
    }

    #[test]
    #[serial]
    fn test_rpc_config_validation_invalid_env() {
        unsafe {
            std::env::set_var("ENV", "invalid");
        }
        let result = RpcConfig::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid ENV value 'invalid'"));
        unsafe {
            std::env::remove_var("ENV");
        }
    }

    #[test]
    #[serial]
    fn test_rpc_config_validation_missing_env() {
        unsafe {
            std::env::remove_var("ENV");
        }
        let result = RpcConfig::from_env();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("ENV environment variable not set")
        );
    }

    #[test]
    #[serial]
    fn test_rpc_config_filters_empty_api_key() {
        unsafe {
            std::env::set_var("ENV", "mainnet");
            std::env::set_var("RPC_API_KEY", "");
        }
        let config = RpcConfig::from_env().unwrap();
        assert!(config.rpc_api_key.is_none());
        unsafe {
            std::env::remove_var("ENV");
            std::env::remove_var("RPC_API_KEY");
        }
    }

    #[test]
    #[serial]
    fn test_rpc_config_filters_empty_alternate_url() {
        unsafe {
            std::env::set_var("ENV", "mainnet");
            std::env::set_var("BEACONATOR_ALTERNATE_RPC", "");
        }
        let config = RpcConfig::from_env().unwrap();
        assert!(config.alternate_rpc_url.is_none());
        unsafe {
            std::env::remove_var("ENV");
            std::env::remove_var("BEACONATOR_ALTERNATE_RPC");
        }
    }
}
