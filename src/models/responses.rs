use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BeaconUpdateResult {
    pub beacon_address: String,
    pub success: bool,
    pub transaction_hash: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchUpdateBeaconResponse {
    pub results: Vec<BeaconUpdateResult>,
    pub total_requested: usize,
    pub successful_updates: usize,
    pub failed_updates: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeployPerpForBeaconResponse {
    pub perp_id: String, // 32-byte pool identifier (e.g., 0x48863de190e7...)
    pub perp_hook_address: String, // 20-byte PerpHook contract address
    pub transaction_hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchCreatePerpcityBeaconResponse {
    pub created_count: u32,
    pub beacon_addresses: Vec<String>,
    pub failed_count: u32,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositLiquidityForPerpResponse {
    pub maker_position_id: String, // Maker position ID from MakerPositionOpened event
    pub approval_transaction_hash: String, // USDC approval transaction hash
    pub deposit_transaction_hash: String, // Liquidity deposit transaction hash
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDepositLiquidityForPerpsResponse {
    pub deposited_count: u32,
    pub maker_position_ids: Vec<String>, // Maker position IDs as strings
    pub failed_count: u32,
    pub errors: Vec<String>,
}
