use alloy::primitives::Bytes;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Update an existing beacon with new data using a zero-knowledge proof
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateBeaconRequest {
    /// Ethereum address of the beacon contract (with or without 0x prefix)
    pub beacon_address: String,
    /// Zero-knowledge proof data as hex string (with 0x prefix)
    #[schemars(with = "String")]
    pub proof: Bytes,
    /// Public signals from the proof as hex string (with 0x prefix), contains the new data value
    #[schemars(with = "String")]
    pub public_signals: Bytes,
}

/// Beacon update data for batch operations
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct BeaconUpdateData {
    /// Ethereum address of the beacon contract (with or without 0x prefix)
    pub beacon_address: String,
    /// Zero-knowledge proof data as hex string (with 0x prefix)
    #[schemars(with = "String")]
    pub proof: Bytes,
    /// Public signals from the proof as hex string (with 0x prefix)
    #[schemars(with = "String")]
    pub public_signals: Bytes,
}

/// Batch update multiple beacons with zero-knowledge proofs
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BatchUpdateBeaconRequest {
    /// List of beacon updates to process
    pub updates: Vec<BeaconUpdateData>,
}

/// Create a beacon by type slug (unified endpoint)
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateBeaconByTypeRequest {
    /// Beacon type slug (e.g., "perpcity", "verifiable-twap")
    pub beacon_type: String,
    /// Type-specific creation parameters (required for Dichotomous factory types)
    pub params: Option<BeaconCreationParams>,
}

/// Type-specific parameters for beacon creation
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BeaconCreationParams {
    /// Verifier contract address (required for Dichotomous factory type)
    pub verifier_address: Option<String>,
    /// Initial data value (required for Dichotomous factory type)
    #[schemars(with = "Option<String>")]
    pub initial_data: Option<u128>,
    /// Initial TWAP observation slots (required for Dichotomous factory type)
    pub initial_cardinality: Option<u32>,
}

/// Batch create beacons by type slug
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BatchCreateBeaconByTypeRequest {
    /// Beacon type slug
    pub beacon_type: String,
    /// Number of beacons to create (1-100)
    pub count: u32,
    /// Type-specific creation parameters (shared across all beacons in batch)
    pub params: Option<BeaconCreationParams>,
}

/// Create a beacon with an auto-deployed ECDSA verifier adapter
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateBeaconWithEcdsaRequest {
    /// Beacon type slug (must reference a Dichotomous factory type)
    pub beacon_type: String,
    /// Initial data value (MUST be pre-scaled by 2^96 if representing a decimal)
    #[schemars(with = "String")]
    pub initial_data: u128,
    /// Initial TWAP observation slots (typically 100-1000)
    pub initial_cardinality: u32,
}

/// Register an existing beacon with the registry
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RegisterBeaconRequest {
    /// Ethereum address of the beacon contract
    pub beacon_address: String,
    /// Ethereum address of the beacon registry contract
    pub registry_address: String,
}

/// Register a new beacon type in the registry
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RegisterBeaconTypeRequest {
    /// Unique slug identifier
    pub slug: String,
    /// Human-readable name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Factory contract address (hex with 0x prefix)
    pub factory_address: String,
    /// Factory interface type
    pub factory_type: crate::models::beacon_type::FactoryType,
    /// Optional registry address for auto-registration (hex with 0x prefix)
    pub registry_address: Option<String>,
    /// Whether this type is enabled (defaults to true)
    pub enabled: Option<bool>,
}

/// Update an existing beacon type configuration
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateBeaconTypeRequest {
    /// Updated human-readable name
    pub name: Option<String>,
    /// Updated description
    pub description: Option<String>,
    /// Updated factory contract address
    pub factory_address: Option<String>,
    /// Updated factory interface type
    pub factory_type: Option<crate::models::beacon_type::FactoryType>,
    /// Updated registry address
    pub registry_address: Option<String>,
    /// Updated enabled status
    pub enabled: Option<bool>,
}

/// Deploy a perpetual contract for a specific beacon
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeployPerpForBeaconRequest {
    /// Ethereum address of the beacon contract
    pub beacon_address: String,
    /// Address of the fees configuration module
    pub fees_module: String,
    /// Address of the margin ratios configuration module
    pub margin_ratios_module: String,
    /// Address of the lockup period configuration module
    pub lockup_period_module: String,
    /// Address of the sqrt price impact limit configuration module
    pub sqrt_price_impact_limit_module: String,
    /// Starting sqrt price in Q96 format as string
    #[schemars(with = "String")]
    pub starting_sqrt_price_x96: String,
}

/// Batch deploy perpetual contracts for multiple beacons
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BatchDeployPerpsForBeaconsRequest {
    /// List of beacon addresses to deploy perps for
    pub beacon_addresses: Vec<String>,
    /// Address of the fees configuration module
    pub fees_module: String,
    /// Address of the margin ratios configuration module
    pub margin_ratios_module: String,
    /// Address of the lockup period configuration module
    pub lockup_period_module: String,
    /// Address of the sqrt price impact limit configuration module
    pub sqrt_price_impact_limit_module: String,
    /// Starting sqrt price in Q96 format as string
    #[schemars(with = "String")]
    pub starting_sqrt_price_x96: String,
}

/// Deposit liquidity for a perpetual contract
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DepositLiquidityForPerpRequest {
    /// Perpetual pool ID as hex string (with or without 0x prefix)
    pub perp_id: String,
    /// USDC margin amount in 6 decimals (e.g., "50000000" for 50 USDC)
    ///
    /// Margin constraints are enforced by on-chain modules. The margin ratios module
    /// defines minimum and maximum allowed margins based on market configuration.
    ///
    /// Current liquidity scaling: margin × 500,000 = final liquidity amount
    pub margin_amount_usdc: String,
    /// Optional holder address (defaults to wallet address if not provided)
    pub holder: Option<String>,
    /// Maximum amount of token0 to deposit (slippage protection), optional
    pub max_amt0_in: Option<String>,
    /// Maximum amount of token1 to deposit (slippage protection), optional
    pub max_amt1_in: Option<String>,
    /// Tick spacing for the liquidity position (defaults to 30)
    pub tick_spacing: Option<i32>,
    /// Lower tick bound for the liquidity position (defaults to 24390)
    pub tick_lower: Option<i32>,
    /// Upper tick bound for the liquidity position (defaults to 53850)
    pub tick_upper: Option<i32>,
}

/// Batch deposit liquidity for multiple perpetual contracts
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BatchDepositLiquidityForPerpsRequest {
    /// List of liquidity deposits to process
    pub liquidity_deposits: Vec<DepositLiquidityForPerpRequest>,
}

/// Fund a guest wallet with USDC and ETH
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FundGuestWalletRequest {
    /// Ethereum address of the wallet to fund
    pub wallet_address: String,
    /// USDC amount in 6 decimals (e.g., "100000000" for 100 USDC)
    pub usdc_amount: String,
    /// ETH amount in wei (e.g., "1000000000000000" for 0.001 ETH)
    pub eth_amount: String,
}

/// Update a beacon using ECDSA signature from the beaconator wallet
///
/// This endpoint signs the measurement with the beaconator wallet and submits
/// it to a beacon that uses an ECDSAVerifierAdapter for verification.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateBeaconWithEcdsaRequest {
    /// Ethereum address of the beacon contract (with or without 0x prefix)
    pub beacon_address: String,
    /// The measurement value to update the beacon with (uint256 as decimal string)
    pub measurement: String,
}
