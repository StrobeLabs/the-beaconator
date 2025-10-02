use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateBeaconRequest {
    pub beacon_address: String,
    pub proof: String,          // ZK proof as hex string
    pub public_signals: String, // Public signals as hex string
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BeaconUpdateData {
    pub beacon_address: String,
    pub proof: String,          // ZK proof as hex string
    pub public_signals: String, // Public signals as hex string
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchUpdateBeaconRequest {
    pub updates: Vec<BeaconUpdateData>,
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
pub struct CreateVerifiableBeaconRequest {
    pub verifier_address: String, // Halo2 verifier contract address
    pub initial_data: u128, // Initial data value (MUST be pre-scaled by 2^96 if representing a decimal)
    pub initial_cardinality: u32, // Initial TWAP observation slots (typically 100-1000)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateVerifiableBeaconRequest {
    pub beacon_address: String, // Address of the verifiable beacon
    pub proof: String,          // ZK proof as hex string
    pub public_signals: String, // Public signals as hex string
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeployPerpForBeaconRequest {
    pub beacon_address: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchCreatePerpcityBeaconRequest {
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositLiquidityForPerpRequest {
    pub perp_id: String, // PoolId as hex string
    /// USDC margin amount in 6 decimals (e.g., "50000000" for 50 USDC)
    ///
    /// **IMPORTANT**: Due to Uniswap V4 liquidity requirements and wide tick range [-23030, 23030],
    /// minimum recommended amount is 10 USDC (10,000,000). Smaller amounts will likely fail
    /// with execution revert due to insufficient liquidity.
    ///
    /// Current scaling: margin × 500,000 = final liquidity amount
    pub margin_amount_usdc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDepositLiquidityForPerpsRequest {
    pub liquidity_deposits: Vec<DepositLiquidityForPerpRequest>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FundGuestWalletRequest {
    pub wallet_address: String,
    pub usdc_amount: String, // Amount in 6 decimals (e.g., "100000000" for 100 USDC)
    pub eth_amount: String,  // Amount in wei (e.g., "1000000000000000" for 0.001 ETH)
}
