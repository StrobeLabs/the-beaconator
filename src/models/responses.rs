use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Standard API response wrapper
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ApiResponse<T> {
    /// Whether the request succeeded
    pub success: bool,
    /// Response data (null if request failed)
    pub data: Option<T>,
    /// Human-readable message about the result
    pub message: String,
}

/// Result of updating a single beacon
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BeaconUpdateResult {
    /// Address of the beacon that was updated
    pub beacon_address: String,
    /// Whether the update succeeded
    pub success: bool,
    /// Transaction hash (if successful)
    pub transaction_hash: Option<String>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Response from batch beacon update operation
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BatchUpdateBeaconResponse {
    /// Individual results for each beacon
    pub results: Vec<BeaconUpdateResult>,
    /// Total number of updates requested
    pub total_requested: usize,
    /// Number of successful updates
    pub successful_updates: usize,
    /// Number of failed updates
    pub failed_updates: usize,
}

/// Response from deploying a perpetual contract
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeployPerpForBeaconResponse {
    /// 32-byte perpetual pool identifier (hex string with 0x prefix)
    pub perp_id: String,
    /// Address of the PerpManager contract
    pub perp_manager_address: String,
    /// Transaction hash
    pub transaction_hash: String,
}

/// Response from batch perpetual deployment
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BatchDeployPerpsForBeaconsResponse {
    /// Number of successfully deployed perpetuals
    pub deployed_count: u32,
    /// List of perpetual pool IDs (hex strings with 0x prefix)
    pub perp_ids: Vec<String>,
    /// Number of failed deployments
    pub failed_count: u32,
    /// Error messages for failed deployments
    pub errors: Vec<String>,
}

/// Response from batch Perpcity beacon creation
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BatchCreatePerpcityBeaconResponse {
    /// Number of successfully created beacons
    pub created_count: u32,
    /// List of beacon addresses (hex strings with 0x prefix)
    pub beacon_addresses: Vec<String>,
    /// Number of failed creations
    pub failed_count: u32,
    /// Error messages for failed creations
    pub errors: Vec<String>,
}

/// Response from depositing liquidity to a perpetual
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DepositLiquidityForPerpResponse {
    /// Maker position ID from MakerPositionOpened event
    pub maker_position_id: String,
    /// USDC approval transaction hash
    pub approval_transaction_hash: String,
    /// Liquidity deposit transaction hash
    pub deposit_transaction_hash: String,
}

/// Response from batch liquidity deposit operation
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BatchDepositLiquidityForPerpsResponse {
    /// Number of successful deposits
    pub deposited_count: u32,
    /// List of maker position IDs
    pub maker_position_ids: Vec<String>,
    /// Number of failed deposits
    pub failed_count: u32,
    /// Error messages for failed deposits
    pub errors: Vec<String>,
}
