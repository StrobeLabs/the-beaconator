use alloy::{json_abi::JsonAbi, primitives::Address};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AlloyProvider;

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
    pub access_token: String,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchDeployPerpsForBeaconsRequest {
    pub beacon_addresses: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchDeployPerpsForBeaconsResponse {
    pub deployed_count: u32,
    pub perp_ids: Vec<String>, // PoolId as hex strings
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
