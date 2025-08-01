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
                description: "Deposit liquidity for a specific perpetual (min: 10 USDC due to wide tick range)".to_string(),
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

impl PerpConfig {
    /// Calculate minimum margin amount based on current configuration.
    ///
    /// This is based on empirical testing with Uniswap V4 and the current tick range.
    /// The calculation considers:
    /// - Wide tick range [-23030, 23030] requires substantial liquidity
    /// - Liquidity scaling factor optimized for reasonable leverage
    /// - Uniswap V4 minimum liquidity thresholds
    ///
    /// Returns minimum margin in USDC (6 decimals)
    pub fn calculate_minimum_margin_usdc(&self) -> u128 {
        // With the new scaling factor, we need much less margin for minimum liquidity
        // Set a reasonable minimum of 10 USDC to allow small positions
        let calculated_min = 10_000_000u128; // 10 USDC in 6 decimals

        // Ensure the minimum doesn't create excessive leverage
        if let Some(leverage) = self.calculate_expected_leverage(calculated_min) {
            let max_leverage = self.max_opening_leverage_x96 as f64 / (2_u128.pow(96) as f64);
            if leverage > max_leverage {
                // If 1 USDC creates too much leverage, find a safe minimum
                let safe_margin = ((max_leverage * 0.8) * calculated_min as f64 / leverage) as u128;
                return std::cmp::max(safe_margin, calculated_min);
            }
        }

        calculated_min
    }

    /// Get user-friendly minimum margin amount in USDC (as decimal)
    pub fn minimum_margin_usdc_decimal(&self) -> f64 {
        self.calculate_minimum_margin_usdc() as f64 / 1_000_000.0
    }

    /// Calculate expected leverage for a given margin amount.
    /// This approximates the relationship: more liquidity = higher leverage (but not linearly)
    /// Returns None if the calculation would result in invalid leverage.
    pub fn calculate_expected_leverage(&self, margin_amount_usdc: u128) -> Option<f64> {
        if margin_amount_usdc == 0 {
            return None;
        }

        // Leverage calculation targeting 10x for 10 USDC taker positions
        // For taker positions, leverage is specified directly, but for maker positions
        // we calculate based on the notional/margin relationship

        let base_margin = 10_000_000f64; // 10 USDC baseline
        let margin_ratio = margin_amount_usdc as f64 / base_margin;

        // Target 10x leverage for 10 USDC, scaling down with sqrt for larger amounts
        // This ensures leverage decreases as margin increases but not linearly
        let target_leverage_10_usdc = 10.0;
        let leverage = target_leverage_10_usdc / margin_ratio.sqrt();

        // Cap leverage at the maximum allowed (9.97x to stay under 10x limit)
        Some(leverage.clamp(0.1, 9.97))
    }

    /// Validate if a margin amount would result in acceptable leverage
    pub fn validate_leverage_bounds(&self, margin_amount_usdc: u128) -> Result<(), String> {
        let expected_leverage = self
            .calculate_expected_leverage(margin_amount_usdc)
            .ok_or("Failed to calculate expected leverage")?;

        let max_leverage = self.max_opening_leverage_x96 as f64 / (2_u128.pow(96) as f64);

        if expected_leverage > max_leverage {
            return Err(format!(
                "Expected leverage {expected_leverage:.2}x exceeds maximum allowed {max_leverage:.2}x. Try reducing margin amount or wait for configuration update."
            ));
        }

        // Check if leverage is too low (below minimum if set)
        if self.min_opening_leverage_x96 > 0 {
            let min_leverage = self.min_opening_leverage_x96 as f64 / (2_u128.pow(96) as f64);
            if expected_leverage < min_leverage {
                return Err(format!(
                    "Expected leverage {expected_leverage:.2}x is below minimum required {min_leverage:.2}x. Try increasing margin amount."
                ));
            }
        }

        Ok(())
    }

    /// Calculate a reasonable maximum margin that stays within leverage bounds
    pub fn calculate_reasonable_max_margin(&self) -> u128 {
        let max_leverage = self.max_opening_leverage_x96 as f64 / (2_u128.pow(96) as f64);
        let tick_range = (self.default_tick_upper - self.default_tick_lower).unsigned_abs() as u128;
        let price_factor = tick_range * 1000;

        // Work backwards: max_leverage = (margin * scaling_factor) / (price_factor)
        // Therefore: margin = (max_leverage * price_factor) / scaling_factor
        let reasonable_margin =
            ((max_leverage * price_factor as f64) / self.liquidity_scaling_factor as f64) as u128;

        // Add some safety buffer (use 80% of calculated max)
        (reasonable_margin * 80) / 100
    }

    /// Calculate minimum and maximum reasonable liquidity for a given margin
    /// Based on the current working configuration and Uniswap V4 constraints
    pub fn calculate_liquidity_bounds(&self, margin_usdc: u128) -> (u128, u128) {
        // Current working scaling factor from contract tests and practical experience
        let current_scaling = self.liquidity_scaling_factor;
        let current_liquidity = margin_usdc * current_scaling;

        // For validation, allow a reasonable range around the current configuration
        // Minimum: 10% of current scaling factor (very conservative)
        let min_liquidity = current_liquidity / 10;

        // Maximum: based on leverage constraint
        // If current config is designed to stay under 10x leverage, allow up to 2x current
        // This gives room for adjustment while preventing excessive leverage
        let max_liquidity = current_liquidity * 2;

        (min_liquidity, max_liquidity)
    }

    /// Validate the PerpConfig parameters for sanity
    pub fn validate(&self) -> Result<(), String> {
        // Check leverage bounds
        if self.min_opening_leverage_x96 > self.max_opening_leverage_x96 {
            return Err(format!(
                "Invalid leverage bounds: min ({}) > max ({})",
                self.min_opening_leverage_x96, self.max_opening_leverage_x96
            ));
        }

        // Check margin bounds
        if self.min_margin_usdc > self.max_margin_usdc {
            return Err(format!(
                "Invalid margin bounds: min ({} USDC) > max ({} USDC)",
                self.min_margin_usdc as f64 / 1_000_000.0,
                self.max_margin_usdc as f64 / 1_000_000.0
            ));
        }

        // Check liquidation leverage vs max opening leverage
        if self.liquidation_leverage_x96 < self.max_opening_leverage_x96 {
            return Err(format!(
                "Liquidation leverage should be >= max opening leverage: liquidation ({}) < max opening ({})",
                self.liquidation_leverage_x96, self.max_opening_leverage_x96
            ));
        }

        // Check tick bounds
        if self.default_tick_lower >= self.default_tick_upper {
            return Err(format!(
                "Invalid tick range: lower ({}) >= upper ({})",
                self.default_tick_lower, self.default_tick_upper
            ));
        }

        // Check tick spacing alignment
        if self.default_tick_lower % self.tick_spacing != 0
            || self.default_tick_upper % self.tick_spacing != 0
        {
            return Err(format!(
                "Ticks not aligned to spacing {}: lower={}, upper={}",
                self.tick_spacing, self.default_tick_lower, self.default_tick_upper
            ));
        }

        // Check calculated minimum vs configured maximum per perp
        let calculated_min = self.calculate_minimum_margin_usdc();
        if calculated_min > self.max_margin_per_perp_usdc {
            return Err(format!(
                "Calculated minimum margin ({} USDC) exceeds maximum per perp ({} USDC). Adjust liquidity_scaling_factor or max_margin_per_perp_usdc.",
                calculated_min as f64 / 1_000_000.0,
                self.max_margin_per_perp_usdc as f64 / 1_000_000.0
            ));
        }

        // Test leverage calculation with minimum margin
        if let Some(min_leverage) = self.calculate_expected_leverage(10_000_000) {
            // 10 USDC
            let max_leverage = self.max_opening_leverage_x96 as f64 / (2_u128.pow(96) as f64);
            if min_leverage > max_leverage {
                return Err(format!(
                    "10 USDC margin produces {min_leverage:.2}x leverage, exceeding max {max_leverage:.2}x. Reduce liquidity_scaling_factor."
                ));
            }
        }

        // Test liquidity bounds for typical margins
        let test_margins = vec![10_000_000u128, 100_000_000u128, 1_000_000_000u128]; // 10, 100, 1000 USDC
        for margin in test_margins {
            let (min_liq, max_liq) = self.calculate_liquidity_bounds(margin);
            let current_liq = margin * self.liquidity_scaling_factor;

            if current_liq < min_liq {
                return Err(format!(
                    "{} USDC margin produces liquidity {} below minimum {} (scaling factor too low)",
                    margin as f64 / 1_000_000.0,
                    current_liq,
                    min_liq
                ));
            }

            if current_liq > max_liq {
                return Err(format!(
                    "{} USDC margin produces liquidity {} above maximum {} (scaling factor too high, will exceed leverage limits)",
                    margin as f64 / 1_000_000.0,
                    current_liq,
                    max_liq
                ));
            }

            tracing::debug!(
                "{} USDC: liquidity {} (bounds: {} - {})",
                margin as f64 / 1_000_000.0,
                current_liq,
                min_liq,
                max_liq
            );
        }

        // Log validation results
        tracing::info!("PerpConfig validation passed:");
        tracing::info!(
            "  - Min margin: {} USDC",
            self.min_margin_usdc as f64 / 1_000_000.0
        );
        tracing::info!(
            "  - Max margin: {} USDC",
            self.max_margin_usdc as f64 / 1_000_000.0
        );
        tracing::info!(
            "  - Max margin per perp: {} USDC",
            self.max_margin_per_perp_usdc as f64 / 1_000_000.0
        );
        tracing::info!(
            "  - Calculated min margin: {} USDC",
            calculated_min as f64 / 1_000_000.0
        );
        tracing::info!(
            "  - Max opening leverage: {:.2}x",
            self.max_opening_leverage_x96 as f64 / (2_u128.pow(96) as f64)
        );
        tracing::info!(
            "  - Liquidation leverage: {:.2}x",
            self.liquidation_leverage_x96 as f64 / (2_u128.pow(96) as f64)
        );
        tracing::info!(
            "  - Liquidity scaling factor: {}",
            self.liquidity_scaling_factor
        );

        if let Some(leverage_10) = self.calculate_expected_leverage(10_000_000) {
            tracing::info!("  - Expected leverage for 10 USDC: {:.2}x", leverage_10);
        }
        if let Some(leverage_100) = self.calculate_expected_leverage(100_000_000) {
            tracing::info!("  - Expected leverage for 100 USDC: {:.2}x", leverage_100);
        }

        Ok(())
    }
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
            default_tick_lower: 24390, // Price ~11.5 (19x range centered on 50)
            default_tick_upper: 53850, // Price ~218 (19x range centered on 50)
            liquidity_scaling_factor: 500_000, // Conservative scaling factor for reasonable leverage
            max_margin_per_perp_usdc: 1_000_000_000, // 1000 USDC in 6 decimals (matching max_margin_usdc)
        }
    }
}

pub struct AppState {
    pub provider: Arc<AlloyProvider>,
    pub alternate_provider: Option<Arc<AlloyProvider>>,
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
    pub perp_id: String, // 32-byte pool identifier (e.g., 0x48863de190e7...)
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
pub struct DepositLiquidityForPerpResponse {
    pub maker_position_id: String, // Maker position ID from MakerPositionOpened event
    pub approval_transaction_hash: String, // USDC approval transaction hash
    pub deposit_transaction_hash: String, // Liquidity deposit transaction hash
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

#[cfg(test)]
#[path = "models_test.rs"]
mod models_test;
