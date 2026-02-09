use alloy::{json_abi::JsonAbi, primitives::Address, signers::local::PrivateKeySigner};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::services::wallet::WalletManager;
use crate::{AlloyProvider, ReadOnlyProvider};

/// API endpoint information for documentation
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EndpointInfo {
    pub method: String,
    pub path: String,
    pub description: String,
    pub requires_auth: bool,
    pub status: EndpointStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum EndpointStatus {
    Working,
    NotImplemented,
    Deprecated,
}

/// Central registry of all API endpoints
pub struct ApiEndpoints;

impl ApiEndpoints {
    pub fn get_all() -> Vec<EndpointInfo> {
        vec![
            EndpointInfo {
                method: "GET".to_string(),
                path: "/".to_string(),
                description: "Welcome page with API documentation".to_string(),
                requires_auth: false,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "GET".to_string(),
                path: "/all_beacons".to_string(),
                description: "List all registered beacons".to_string(),
                requires_auth: true,
                status: EndpointStatus::NotImplemented,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/create_beacon".to_string(),
                description: "Create a new beacon".to_string(),
                requires_auth: true,
                status: EndpointStatus::NotImplemented,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/register_beacon".to_string(),
                description: "Register an existing beacon".to_string(),
                requires_auth: true,
                status: EndpointStatus::NotImplemented,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/create_perpcity_beacon".to_string(),
                description: "Create and register a new Perpcity beacon".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/batch_create_perpcity_beacon".to_string(),
                description: "Batch create multiple Perpcity beacons".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/deploy_perp_for_beacon".to_string(),
                description: "Deploy a perpetual for a specific beacon".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/batch_deploy_perps_for_beacons".to_string(),
                description: "Batch deploy perpetuals for multiple beacons".to_string(),
                requires_auth: true,
                status: EndpointStatus::NotImplemented,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/deposit_liquidity_for_perp".to_string(),
                description: "Deposit liquidity for a specific perpetual (min: 10 USDC due to wide tick range)".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/batch_deposit_liquidity_for_perps".to_string(),
                description: "Batch deposit liquidity for multiple perpetuals".to_string(),
                requires_auth: true,
                status: EndpointStatus::NotImplemented,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/update_beacon".to_string(),
                description: "Update beacon data (supports both ownable and verifiable beacons)".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/batch_update_beacon".to_string(),
                description: "Batch update multiple beacons with zero-knowledge proofs".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/fund_guest_wallet".to_string(),
                description: "Fund a guest wallet with specified USDC + ETH amounts (with limits)"
                    .to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/create_verifiable_beacon".to_string(),
                description: "Create a verifiable beacon with ZK proof support and TWAP".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/update_beacon_with_ecdsa_adapter".to_string(),
                description: "Update a beacon using ECDSA signature from the beaconator wallet"
                    .to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
        ]
    }

    pub fn get_summary() -> ApiSummary {
        let endpoints = Self::get_all();
        let total = endpoints.len();
        let working = endpoints
            .iter()
            .filter(|e| matches!(e.status, EndpointStatus::Working))
            .count();
        let not_implemented = endpoints
            .iter()
            .filter(|e| matches!(e.status, EndpointStatus::NotImplemented))
            .count();
        let deprecated = endpoints
            .iter()
            .filter(|e| matches!(e.status, EndpointStatus::Deprecated))
            .count();

        ApiSummary {
            total_endpoints: total,
            working_endpoints: working,
            not_implemented,
            deprecated,
            endpoints,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApiSummary {
    pub total_endpoints: usize,
    pub working_endpoints: usize,
    pub not_implemented: usize,
    pub deprecated: usize,
    pub endpoints: Vec<EndpointInfo>,
}

#[derive(Clone)]
pub struct AppState {
    // Read-only provider for queries (no wallet)
    pub read_provider: Arc<ReadOnlyProvider>,

    // Funding provider (PRIVATE_KEY wallet) - ONLY for fund_guest_wallet
    pub funding_provider: Arc<AlloyProvider>,
    pub funding_wallet_address: Address,

    // WalletManager for contract operations (Turnkey wallets)
    pub wallet_manager: Arc<WalletManager>,

    // RPC configuration for building providers from WalletHandle
    pub rpc_url: String,
    pub chain_id: u64,

    /// Signer from PRIVATE_KEY - used for ECDSA beacon signatures.
    /// This wallet's address must match the designated signer configured
    /// in each ECDSA beacon's verifier adapter.
    pub signer: PrivateKeySigner,

    // Contract ABIs
    pub beacon_abi: JsonAbi,
    pub beacon_factory_abi: JsonAbi,
    pub beacon_registry_abi: JsonAbi,
    pub perp_manager_abi: JsonAbi,
    pub multicall3_abi: JsonAbi,
    pub dichotomous_beacon_factory_abi: JsonAbi,
    pub step_beacon_abi: JsonAbi,
    pub ecdsa_beacon_abi: JsonAbi,
    pub ecdsa_verifier_adapter_abi: JsonAbi,

    // Contract addresses
    pub beacon_factory_address: Address,
    pub perpcity_registry_address: Address,
    pub perp_manager_address: Address,
    pub usdc_address: Address,
    pub dichotomous_beacon_factory_address: Option<Address>,

    // Transfer limits
    pub usdc_transfer_limit: u128,
    pub eth_transfer_limit: u128,

    // Authentication
    pub access_token: String,

    // Perp module addresses
    pub fees_module_address: Address,
    pub margin_ratios_module_address: Address,
    pub lockup_period_module_address: Address,
    pub sqrt_price_impact_limit_module_address: Address,
    pub default_starting_sqrt_price_x96: Option<u128>,

    // Optional multicall3 contract for batch operations
    pub multicall3_address: Option<Address>,
}
