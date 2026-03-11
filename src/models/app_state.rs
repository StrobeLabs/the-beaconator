use alloy::{
    primitives::{Address, Bytes},
    signers::local::PrivateKeySigner,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::ReadOnlyProvider;
use crate::services::beacon::BeaconTypeRegistry;
use crate::services::beacon::ComponentFactoryRegistry;
use crate::services::beacon::RecipeRegistry;
use crate::services::wallet::WalletManager;

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
                method: "POST".to_string(),
                path: "/create_beacon".to_string(),
                description: "Create a beacon by type slug (unified endpoint)".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/create_beacon_with_ecdsa".to_string(),
                description: "Create an IdentityBeacon with auto-deployed ECDSA verifier".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/register_beacon".to_string(),
                description: "Register an existing beacon with a registry".to_string(),
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
                path: "/deposit_liquidity_for_perp".to_string(),
                description: "Deposit liquidity for a specific perpetual (min: 10 USDC due to wide tick range)".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
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
                method: "GET".to_string(),
                path: "/beacon_types".to_string(),
                description: "List all registered beacon types (admin)".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "GET".to_string(),
                path: "/beacon_type/<slug>".to_string(),
                description: "Get a beacon type by slug (admin)".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/beacon_types".to_string(),
                description: "Register a new beacon type (admin)".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "PUT".to_string(),
                path: "/beacon_type/<slug>".to_string(),
                description: "Update a beacon type (admin)".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "DELETE".to_string(),
                path: "/beacon_type/<slug>".to_string(),
                description: "Delete a beacon type (admin)".to_string(),
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
    pub provider: ProviderConfig,
    pub wallets: WalletConfig,
    pub contracts: ContractAddresses,
    pub auth: AuthConfig,
    pub registries: Registries,
}

#[derive(Clone)]
pub struct ProviderConfig {
    pub read_provider: Arc<ReadOnlyProvider>,
    pub rpc_url: String,
    pub chain_id: u64,
}

#[derive(Clone)]
pub struct WalletConfig {
    pub manager: Arc<WalletManager>,
    pub funding_address: Address,
    /// Signer from PRIVATE_KEY - used for ECDSA beacon signatures.
    /// This wallet's address must match the designated signer configured
    /// in each ECDSA beacon's verifier adapter.
    pub signer: PrivateKeySigner,
    pub usdc_transfer_limit: u128,
    pub eth_transfer_limit: u128,
}

#[derive(Clone)]
pub struct ContractAddresses {
    pub perpcity_registry: Address,
    pub perp_manager: Address,
    pub usdc: Address,
    pub ecdsa_verifier_factory: Address,
    pub multicall3: Option<Address>,
    pub identity_beacon_bytecode: Bytes,
    pub safe: Option<SafeConfig>,
}

#[derive(Clone)]
pub struct SafeConfig {
    pub address: Address,
    pub tx_service_url: Option<String>,
}

#[derive(Clone)]
pub struct AuthConfig {
    pub access_token: String,
    pub admin_token: String,
}

#[derive(Clone)]
pub struct Registries {
    pub beacon_types: Arc<BeaconTypeRegistry>,
    pub component_factories: Arc<ComponentFactoryRegistry>,
    pub recipes: Arc<RecipeRegistry>,
}
