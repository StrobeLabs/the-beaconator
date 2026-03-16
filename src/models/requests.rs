use alloy::primitives::Bytes;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

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
    /// Beacon type slug (e.g., "identity")
    pub beacon_type: String,
    /// Type-specific creation parameters
    pub params: Option<BeaconCreationParams>,
}

/// Type-specific parameters for beacon creation
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BeaconCreationParams {
    /// Initial beacon index value
    #[schemars(with = "Option<String>")]
    pub initial_index: Option<u128>,
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

/// Create an IdentityBeacon with an auto-deployed ECDSA verifier
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateBeaconWithEcdsaRequest {
    /// Initial beacon index value
    #[schemars(with = "String")]
    pub initial_index: u128,
}

/// Create an LBCGBM standalone beacon via the LBCGBMFactory
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateLBCGBMBeaconRequest {
    /// Measurement scale for the Identity preprocessor
    #[schemars(with = "String")]
    pub measurement_scale: u128,
    /// Base sigma for CGBM
    #[schemars(with = "String")]
    pub sigma_base: u128,
    /// Scaling factor for CGBM
    #[schemars(with = "String")]
    pub scaling_factor: u128,
    /// Alpha parameter for CGBM
    #[schemars(with = "String")]
    pub alpha: u128,
    /// Decay parameter for CGBM
    #[schemars(with = "String")]
    pub decay: u128,
    /// Initial sigma ratio for CGBM
    #[schemars(with = "String")]
    pub initial_sigma_ratio: u128,
    /// Whether to use variance scaling in CGBM
    pub variance_scaling: bool,
    /// Minimum index for the Bounded transform
    #[schemars(with = "String")]
    pub min_index: u128,
    /// Maximum index for the Bounded transform
    #[schemars(with = "String")]
    pub max_index: u128,
    /// Steepness for the Bounded transform sigmoid
    #[schemars(with = "String")]
    pub steepness: u128,
    /// Initial beacon index value
    #[schemars(with = "String")]
    pub initial_index: u128,
}

/// Create a WeightedSumComposite beacon via the WeightedSumCompositeFactory
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateWeightedSumCompositeBeaconRequest {
    /// Addresses of reference beacons to compose (hex with 0x prefix)
    pub reference_beacons: Vec<String>,
    /// Weights for the WeightedSum composer (WAD-scaled, as decimal strings)
    #[schemars(with = "Vec<String>")]
    pub weights: Vec<u128>,
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
///
/// The `measurement` field accepts either:
/// - A single uint256 string: `"measurement": "1000000000000000000"` (standalone beacons)
/// - An array of uint256 strings: `"measurement": ["47941000000000000", ...]` (group beacons)
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdateBeaconWithEcdsaRequest {
    /// Ethereum address of the beacon contract (with or without 0x prefix)
    pub beacon_address: String,
    /// Measurement value(s) as uint256 decimal string(s).
    /// A single string is treated as a one-element array for backwards compatibility.
    #[serde(deserialize_with = "deserialize_measurement")]
    #[schemars(with = "MeasurementInput")]
    pub measurement: Vec<String>,
}

/// Schema type for the measurement field: accepts a single string or an array of strings.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
enum MeasurementInput {
    Single(String),
    Multiple(Vec<String>),
}

/// Deserialize measurement as either a single string or a vec of strings.
fn deserialize_measurement<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    match MeasurementInput::deserialize(deserializer)? {
        MeasurementInput::Single(s) => Ok(vec![s]),
        MeasurementInput::Multiple(v) => Ok(v),
    }
}

/// Create a modular beacon using a named recipe
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CreateModularBeaconRequest {
    /// Recipe slug (e.g., "lbcgbm", "dgbm", "dominance")
    pub recipe: String,
    /// Component-specific parameters (recipe determines which fields are required)
    pub params: ModularBeaconParams,
}

/// Parameters for modular beacon creation. All fields are optional - the recipe determines which are required.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ModularBeaconParams {
    // -- Preprocessor params --
    /// Measurement scale for preprocessor (WAD-scaled)
    #[schemars(with = "Option<String>")]
    pub measurement_scale: Option<u128>,
    /// Threshold value for Threshold/TernaryToBinary preprocessors (WAD-scaled)
    #[schemars(with = "Option<String>")]
    pub threshold: Option<u128>,

    // -- CGBM base function params --
    /// Base sigma for CGBM/DGBM
    #[schemars(with = "Option<String>")]
    pub sigma_base: Option<u128>,
    /// Scaling factor for CGBM/DGBM
    #[schemars(with = "Option<String>")]
    pub scaling_factor: Option<u128>,
    /// Alpha parameter for CGBM power law exponent
    #[schemars(with = "Option<String>")]
    pub alpha: Option<u128>,
    /// Decay parameter (EMA decay factor, WAD-scaled)
    #[schemars(with = "Option<String>")]
    pub decay: Option<u128>,
    /// Initial sigma ratio for CGBM
    #[schemars(with = "Option<String>")]
    pub initial_sigma_ratio: Option<u128>,
    /// Whether to use variance scaling in CGBM
    pub variance_scaling: Option<bool>,

    // -- DGBM-specific params --
    /// Initial positive rate for DGBM (WAD-scaled)
    #[schemars(with = "Option<String>")]
    pub initial_positive_rate: Option<u128>,

    // -- Bounded transform params --
    /// Minimum index for Bounded transform
    #[schemars(with = "Option<String>")]
    pub min_index: Option<u128>,
    /// Maximum index for Bounded transform
    #[schemars(with = "Option<String>")]
    pub max_index: Option<u128>,
    /// Steepness for Bounded/Softmax transform sigmoid
    #[schemars(with = "Option<String>")]
    pub steepness: Option<u128>,

    // -- Beacon params --
    /// Initial beacon index value
    #[schemars(with = "Option<String>")]
    pub initial_index: Option<u128>,

    // -- Composite params --
    /// Addresses of reference beacons for composite (hex with 0x prefix)
    pub reference_beacons: Option<Vec<String>>,
    /// Weights for WeightedSum composer (WAD-scaled)
    #[schemars(with = "Option<Vec<String>>")]
    pub weights: Option<Vec<u128>>,

    // -- Group params --
    /// Number of classes for group functions
    #[schemars(with = "Option<String>")]
    pub num_classes: Option<u128>,
    /// Class probabilities for allocation group functions (WAD-scaled)
    #[schemars(with = "Option<Vec<String>>")]
    pub class_probs: Option<Vec<u128>>,
    /// Initial indices for group manager members
    #[schemars(with = "Option<Vec<String>>")]
    pub initial_indices: Option<Vec<u128>>,
    /// Initial z-space indices for group manager members
    #[schemars(with = "Option<Vec<String>>")]
    pub initial_z_space_indices: Option<Vec<i128>>,
    /// Initial EMA values for Dominance group function
    #[schemars(with = "Option<Vec<String>>")]
    pub initial_ema: Option<Vec<u128>>,
    /// Fast decay for RelativeDominance (WAD-scaled)
    #[schemars(with = "Option<String>")]
    pub decay_fast: Option<u128>,
    /// Slow decay for RelativeDominance (WAD-scaled)
    #[schemars(with = "Option<String>")]
    pub decay_slow: Option<u128>,
    /// Initial fast EMA for RelativeDominance
    #[schemars(with = "Option<Vec<String>>")]
    pub initial_m_fast: Option<Vec<u128>>,
    /// Initial slow EMA for RelativeDominance
    #[schemars(with = "Option<Vec<String>>")]
    pub initial_m_slow: Option<Vec<u128>>,
}
