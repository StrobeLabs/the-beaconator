use alloy::primitives::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateBeaconRequest {
    pub beacon_address: String,
    pub proof: Bytes,          // 0x-hex in JSON
    pub public_signals: Bytes, // 0x-hex in JSON (contains the new data value)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BeaconUpdateData {
    pub beacon_address: String,
    pub proof: Bytes,          // 0x-hex in JSON
    pub public_signals: Bytes, // 0x-hex in JSON
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
    pub beacon_address: String,
    pub registry_address: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateVerifiableBeaconRequest {
    pub verifier_address: String, // Halo2 verifier contract address
    pub initial_data: u128, // Initial data value (MUST be pre-scaled by 2^96 if representing a decimal)
    pub initial_cardinality: u32, // Initial TWAP observation slots (typically 100-1000)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeployPerpForBeaconRequest {
    pub beacon_address: String,
    pub fees_module: String,
    pub margin_ratios_module: String,
    pub lockup_period_module: String,
    pub sqrt_price_impact_limit_module: String,
    pub starting_sqrt_price_x96: String, // Q96 format as string
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchDeployPerpsForBeaconsRequest {
    pub beacon_addresses: Vec<String>,
    pub fees_module: String,
    pub margin_ratios_module: String,
    pub lockup_period_module: String,
    pub sqrt_price_impact_limit_module: String,
    pub starting_sqrt_price_x96: String, // Q96 format as string
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
    /// Margin constraints are enforced by on-chain modules. The margin ratios module
    /// defines minimum and maximum allowed margins based on market configuration.
    ///
    /// Current liquidity scaling: margin × 500,000 = final liquidity amount
    pub margin_amount_usdc: String,
    /// Optional holder address (defaults to wallet address if not provided)
    pub holder: Option<String>,
    /// Maximum amount of token0 to deposit (slippage protection)
    /// Optional - defaults to a reasonable max if not provided
    pub max_amt0_in: Option<String>,
    /// Maximum amount of token1 to deposit (slippage protection)
    /// Optional - defaults to a reasonable max if not provided
    pub max_amt1_in: Option<String>,
    /// Tick spacing for the liquidity position
    /// Optional - defaults to 30 if not provided
    pub tick_spacing: Option<i32>,
    /// Lower tick bound for the liquidity position
    /// Optional - defaults to 24390 if not provided
    pub tick_lower: Option<i32>,
    /// Upper tick bound for the liquidity position
    /// Optional - defaults to 53850 if not provided
    pub tick_upper: Option<i32>,
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
