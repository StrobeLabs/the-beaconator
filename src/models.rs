use alloy::{json_abi::JsonAbi, primitives::Address};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AlloyProvider;

/// API endpoint information for documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointInfo {
    pub method: String,
    pub path: String,
    pub description: String,
    pub requires_auth: bool,
    pub status: EndpointStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
                path: "/deposit_liquidity_for_perp".to_string(),
                description: "Deposit liquidity for a specific perpetual".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/batch_deposit_liquidity_for_perps".to_string(),
                description: "Batch deposit liquidity for multiple perpetuals".to_string(),
                requires_auth: true,
                status: EndpointStatus::Working,
            },
            EndpointInfo {
                method: "POST".to_string(),
                path: "/update_beacon".to_string(),
                description: "Update beacon data with zero-knowledge proof".to_string(),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSummary {
    pub total_endpoints: usize,
    pub working_endpoints: usize,
    pub not_implemented: usize,
    pub deprecated: usize,
    pub endpoints: Vec<EndpointInfo>,
}

/// Configuration for perpetual contract parameters
#[derive(Debug, Clone)]
pub struct PerpConfig {
    /// Trading fee in basis points (e.g., 5000 = 0.5%)
    pub trading_fee_bps: u32,
    /// Minimum margin amount in USDC (6 decimals)
    pub min_margin_usdc: u128,
    /// Maximum margin amount in USDC (6 decimals, e.g., 1000 USDC = 1_000_000_000)
    pub max_margin_usdc: u128,
    /// Minimum opening leverage in Q96 format (0 = no minimum)
    pub min_opening_leverage_x96: u128,
    /// Maximum opening leverage in Q96 format (e.g., 10x = 790273926286361721684336819027)
    pub max_opening_leverage_x96: u128,
    /// Liquidation leverage threshold in Q96 format (e.g., 10x = 790273926286361721684336819027)
    pub liquidation_leverage_x96: u128,
    /// Liquidation fee percentage in Q96 format (e.g., 1% = 790273926286361721684336819)
    pub liquidation_fee_x96: u128,
    /// Liquidation fee split percentage in Q96 format (e.g., 50% = 39513699123034658136834084095)
    pub liquidation_fee_split_x96: u128,
    /// Funding interval in seconds (e.g., 86400 = 1 day)
    pub funding_interval_seconds: i128,
    /// Tick spacing for price ticks (e.g., 30)
    pub tick_spacing: i32,
    /// Starting square root price in Q96 format (e.g., sqrt(50) * 2^96 = 560227709747861419891227623424)
    pub starting_sqrt_price_x96: u128,
    /// Default tick range for liquidity positions - lower bound (e.g., -23030 ≈ sqrt(0.1) price)
    pub default_tick_lower: i32,
    /// Default tick range for liquidity positions - upper bound (e.g., 23030 ≈ sqrt(10) price)
    pub default_tick_upper: i32,
    /// Liquidity scaling factor (multiplier to convert USDC margin to 18-decimal liquidity)
    pub liquidity_scaling_factor: u128,
    /// Maximum margin amount per perp in USDC (6 decimals)
    pub max_margin_per_perp_usdc: u128,
}

impl Default for PerpConfig {
    fn default() -> Self {
        // Values that exactly match DeployPerp.s.sol constants
        // These are calculated values that match the Solidity script

        Self {
            trading_fee_bps: 5000,          // TRADING_FEE = 5000 (0.5%)
            min_margin_usdc: 0,             // MIN_MARGIN = 0
            max_margin_usdc: 1_000_000_000, // MAX_MARGIN = 1000e6 (1000 USDC in 6 decimals)
            min_opening_leverage_x96: 0,    // MIN_OPENING_LEVERAGE_X96 = 0
            max_opening_leverage_x96: 790273926286361721684336819027, // MAX_OPENING_LEVERAGE_X96 = (10 * FixedPoint96.Q96).toUint128()
            liquidation_leverage_x96: 790273926286361721684336819027, // LIQUIDATION_LEVERAGE_X96 = (10 * FixedPoint96.Q96).toUint128()
            liquidation_fee_x96: 790273926286361721684336819, // LIQUIDATION_FEE_X96 = (1 * FixedPoint96.Q96 / 100).toUint128()
            liquidation_fee_split_x96: 39513699123034658136834084095, // LIQUIDATION_FEE_SPLIT_X96 = (50 * FixedPoint96.Q96 / 100).toUint128()
            funding_interval_seconds: 86400, // FUNDING_INTERVAL = 1 days = 86400 seconds
            tick_spacing: 30,                // TICK_SPACING = 30
            starting_sqrt_price_x96: 560227709747861419891227623424, // STARTING_SQRT_PRICE_X96 = SQRT_50_X96 = 2^96 * sqrt(50)
            default_tick_lower: -23030,                              // Approx sqrt(0.1) price
            default_tick_upper: 23030,                               // Approx sqrt(10) price
            liquidity_scaling_factor: 400_000_000_000_000,           // Scale USDC to 18 decimals
            max_margin_per_perp_usdc: 5_000_000,                     // 5 USDC in 6 decimals
        }
    }
}

pub struct AppState {
    pub provider: Arc<AlloyProvider>,
    pub wallet_address: Address,
    pub beacon_abi: JsonAbi,
    pub beacon_factory_abi: JsonAbi,
    pub beacon_registry_abi: JsonAbi,
    pub perp_hook_abi: JsonAbi,
    pub beacon_factory_address: Address,
    pub perpcity_registry_address: Address,
    pub perp_hook_address: Address,
    pub usdc_address: Address,
    pub usdc_transfer_limit: u128,
    pub eth_transfer_limit: u128,
    pub access_token: String,
    pub perp_config: PerpConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateBeaconRequest {
    pub beacon_address: String,
    pub value: u64,
    pub proof: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateBeaconRequest {
    // TODO: Define the fields needed for creating a beacon
    pub placeholder: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterBeaconRequest {
    // TODO: Define the fields needed for registering a beacon
    pub placeholder: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeployPerpForBeaconRequest {
    pub beacon_address: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeployPerpForBeaconResponse {
    pub perp_id: String,           // 32-byte pool identifier (e.g., 0x48863de190e7...)
    pub perp_hook_address: String, // 20-byte PerpHook contract address
    pub transaction_hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchCreatePerpcityBeaconRequest {
    pub count: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchCreatePerpcityBeaconResponse {
    pub created_count: u32,
    pub beacon_addresses: Vec<String>,
    pub failed_count: u32,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositLiquidityForPerpRequest {
    pub perp_id: String,            // PoolId as hex string
    pub margin_amount_usdc: String, // USDC amount in 6 decimals (e.g., "500000000" for 500 USDC)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDepositLiquidityForPerpsRequest {
    pub liquidity_deposits: Vec<DepositLiquidityForPerpRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDepositLiquidityForPerpsResponse {
    pub deposited_count: u32,
    pub maker_position_ids: Vec<String>, // Maker position IDs as strings
    pub failed_count: u32,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FundGuestWalletRequest {
    pub wallet_address: String,
    pub usdc_amount: String, // Amount in 6 decimals (e.g., "100000000" for 100 USDC)
    pub eth_amount: String,  // Amount in wei (e.g., "1000000000000000" for 0.001 ETH)
}
