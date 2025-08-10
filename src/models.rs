use alloy::{
    json_abi::JsonAbi,
    primitives::{Address, U256},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AlloyProvider;

/// Q96 constant for fixed point math (2^96)
/// Used for Uniswap V4 price and liquidity calculations
pub const Q96: u128 = 79228162514264337593543950336;

/// TickMath constants for min/max ticks
pub const MIN_TICK: i32 = -887272;
pub const MAX_TICK: i32 = 887272;

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
    /// Default tick range for liquidity positions - lower bound (40950 ≈ price 35.7)
    pub default_tick_lower: i32,
    /// Default tick range for liquidity positions - upper bound (46050 ≈ price 70.1)
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
    /// - Tick range [40950, 46050] provides concentrated liquidity
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

    /// Convert tick to sqrt price X96
    ///
    /// This implements Uniswap's exact integer-based TickMath.getSqrtRatioAtTick algorithm
    /// to calculate sqrt(1.0001^tick) as a Q64.96 fixed point number.
    ///
    /// # Panics
    /// Panics if tick is outside the valid range [MIN_TICK, MAX_TICK]
    pub fn tick_to_sqrt_price_x96(tick: i32) -> u128 {
        // Check tick bounds
        if !(MIN_TICK..=MAX_TICK).contains(&tick) {
            panic!("Tick {tick} is outside valid range [{MIN_TICK}, {MAX_TICK}]");
        }

        // Get absolute value of tick
        let abs_tick = if tick < 0 {
            (-tick) as u32
        } else {
            tick as u32
        };

        // Initialize ratio as Q128.128 fixed point number
        // Using Uniswap's exact constants
        let mut ratio = if abs_tick & 0x1 != 0 {
            U256::from_be_bytes([
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0xff, 0xfc, 0xb9, 0x33, 0xbd, 0x6f, 0xad, 0x37, 0xaa, 0x2d, 0x16, 0x2d,
                0x1a, 0x59, 0x40, 0x01,
            ])
        } else {
            U256::from(1) << 128
        };

        // Binary search through bits using Uniswap's constants
        if abs_tick & 0x2 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0xff, 0xf9, 0x72, 0x72, 0x37, 0x3d, 0x41, 0x32, 0x59, 0xa4,
                    0x69, 0x90, 0x58, 0x0e, 0x21, 0x3a,
                ]))
                >> 128;
        }
        if abs_tick & 0x4 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0xff, 0xf2, 0xe5, 0x0f, 0x5f, 0x65, 0x69, 0x32, 0xef, 0x12,
                    0x35, 0x7c, 0xf3, 0xc7, 0xfd, 0xcc,
                ]))
                >> 128;
        }
        if abs_tick & 0x8 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0xff, 0xe5, 0xca, 0xca, 0x7e, 0x10, 0xe4, 0xe6, 0x1c, 0x36,
                    0x24, 0xea, 0xa0, 0x94, 0x1c, 0xd0,
                ]))
                >> 128;
        }
        if abs_tick & 0x10 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0xff, 0xcb, 0x98, 0x43, 0xd6, 0x0f, 0x61, 0x59, 0xc9, 0xdb,
                    0x58, 0x83, 0x5c, 0x92, 0x66, 0x44,
                ]))
                >> 128;
        }
        if abs_tick & 0x20 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0xff, 0x97, 0x3b, 0x41, 0xfa, 0x98, 0xc0, 0x81, 0x47, 0x2e,
                    0x68, 0x96, 0xdf, 0xb2, 0x54, 0xc0,
                ]))
                >> 128;
        }
        if abs_tick & 0x40 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0xff, 0x2e, 0xa1, 0x64, 0x66, 0xc9, 0x6a, 0x38, 0x43, 0xec,
                    0x78, 0xb3, 0x26, 0xb5, 0x28, 0x61,
                ]))
                >> 128;
        }
        if abs_tick & 0x80 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0xfe, 0x5d, 0xee, 0x04, 0x6a, 0x99, 0xa2, 0xa8, 0x11, 0xc4,
                    0x61, 0xf1, 0x96, 0x9c, 0x30, 0x53,
                ]))
                >> 128;
        }
        if abs_tick & 0x100 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0xfc, 0xbe, 0x86, 0xc7, 0x90, 0x0a, 0x88, 0xae, 0xdc, 0xff,
                    0xc8, 0x3b, 0x47, 0x9a, 0xa3, 0xa4,
                ]))
                >> 128;
        }
        if abs_tick & 0x200 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0xf9, 0x87, 0xa7, 0x25, 0x3a, 0xc4, 0x13, 0x17, 0x6f, 0x2b,
                    0x07, 0x4c, 0xf7, 0x81, 0x5e, 0x54,
                ]))
                >> 128;
        }
        if abs_tick & 0x400 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0xf3, 0x39, 0x2b, 0x08, 0x22, 0xb7, 0x00, 0x05, 0x94, 0x0c,
                    0x7a, 0x39, 0x8e, 0x4b, 0x70, 0xf3,
                ]))
                >> 128;
        }
        if abs_tick & 0x800 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0xe7, 0x15, 0x94, 0x75, 0xa2, 0xc2, 0x9b, 0x74, 0x43, 0xb2,
                    0x9c, 0x7f, 0xa6, 0xe8, 0x89, 0xd9,
                ]))
                >> 128;
        }
        if abs_tick & 0x1000 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0xd0, 0x97, 0xf3, 0xbd, 0xfd, 0x20, 0x22, 0xb8, 0x84, 0x5a,
                    0xd8, 0xf7, 0x92, 0xaa, 0x58, 0x25,
                ]))
                >> 128;
        }
        if abs_tick & 0x2000 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0xa9, 0xf7, 0x46, 0x46, 0x2d, 0x87, 0x0f, 0xdf, 0x8a, 0x65,
                    0xdc, 0x1f, 0x90, 0xe0, 0x61, 0xe5,
                ]))
                >> 128;
        }
        if abs_tick & 0x4000 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x70, 0xd8, 0x69, 0xa1, 0x56, 0xd2, 0xa1, 0xb8, 0x90, 0xbb,
                    0x3d, 0xf6, 0x2b, 0xaf, 0x32, 0xf7,
                ]))
                >> 128;
        }
        if abs_tick & 0x8000 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x31, 0xbe, 0x13, 0x5f, 0x97, 0xd0, 0x8f, 0xd9, 0x81, 0x23,
                    0x15, 0x05, 0x54, 0x2f, 0xcf, 0xa6,
                ]))
                >> 128;
        }
        if abs_tick & 0x10000 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x09, 0xaa, 0x50, 0x8b, 0x5b, 0x7a, 0x84, 0xe1, 0xc6, 0x77,
                    0xde, 0x54, 0xf3, 0xe9, 0x9b, 0xc9,
                ]))
                >> 128;
        }
        if abs_tick & 0x20000 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x5d, 0x6a, 0xf8, 0xde, 0xdb, 0x81, 0x19, 0x66, 0x99,
                    0xc3, 0x29, 0x22, 0x5e, 0xe6, 0x04,
                ]))
                >> 128;
        }
        if abs_tick & 0x40000 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x02, 0x21, 0x6e, 0x58, 0x4f, 0x5f, 0xa1, 0xea, 0x92,
                    0x60, 0x41, 0xbe, 0xdf, 0xe9, 0x98,
                ]))
                >> 128;
        }
        if abs_tick & 0x80000 != 0 {
            ratio = (ratio
                * U256::from_be_bytes([
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xa1, 0x70, 0x39, 0x1f, 0x7d, 0xc4,
                    0x24, 0x44, 0xe8, 0xfa, 0x20,
                ]))
                >> 128;
        }

        // Convert from Q128.128 to Q64.96
        // If tick > 0, ratio = 1 / ratio
        if tick > 0 {
            ratio = U256::MAX / ratio;
        }

        // Shift to convert to Q64.96 and round up if necessary
        let sqrt_price_x96: U256 = (ratio >> 32)
            + if (ratio % (U256::from(1) << 32)) == U256::ZERO {
                U256::ZERO
            } else {
                U256::from(1)
            };

        // Convert to u128, should always fit as we're in Q64.96
        sqrt_price_x96.to::<u128>()
    }

    /// Calculate liquidity for amount1 using Uniswap V4 formula
    /// Replicates LiquidityAmounts.getLiquidityForAmount1
    pub fn get_liquidity_for_amount1(
        sqrt_price_a_x96: u128,
        sqrt_price_b_x96: u128,
        amount1: u128,
    ) -> U256 {
        // Convert to U256 for big number math
        let sqrt_a_u256 = U256::from(sqrt_price_a_x96);
        let sqrt_b_u256 = U256::from(sqrt_price_b_x96);
        let amount1_u256 = U256::from(amount1);

        // Ensure sqrtPriceAX96 <= sqrtPriceBX96
        let (sqrt_price_lower, sqrt_price_upper) = if sqrt_a_u256 > sqrt_b_u256 {
            (sqrt_b_u256, sqrt_a_u256)
        } else {
            (sqrt_a_u256, sqrt_b_u256)
        };

        // liquidity = amount1 * Q96 / (sqrtPriceUpperX96 - sqrtPriceLowerX96)
        let denominator = sqrt_price_upper - sqrt_price_lower;
        if denominator == U256::ZERO {
            return U256::ZERO;
        }

        // Calculate using U256 to avoid overflow
        // Multiply by 2^96 using left shift
        let numerator = amount1_u256 << 96;

        // Return liquidity as U256
        numerator / denominator
    }

    /// Calculate liquidity based on margin amount and configured tick range
    pub fn calculate_liquidity_from_margin(&self, margin_amount_usdc: u128) -> u128 {
        // Convert ticks to sqrt prices
        let sqrt_price_lower_x96 = Self::tick_to_sqrt_price_x96(self.default_tick_lower);
        let sqrt_price_upper_x96 = Self::tick_to_sqrt_price_x96(self.default_tick_upper);

        // Convert USDC (6 decimals) to 18 decimals
        let amount1_18_decimals = margin_amount_usdc * 10_u128.pow(12);

        // Use the Uniswap formula and convert result back to u128
        // This is safe because we control the input ranges
        let liquidity_u256 = Self::get_liquidity_for_amount1(
            sqrt_price_lower_x96,
            sqrt_price_upper_x96,
            amount1_18_decimals,
        );

        // Convert to u128, saturating if too large (though this shouldn't happen with our ranges)
        liquidity_u256.saturating_to::<u128>()
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
        // With the new Uniswap liquidity calculation, we need a different approach
        // The leverage calculation is complex and depends on liquidity, notional, and tick range
        // For now, return a reasonable value based on our max margin configuration

        // Use 80% of the configured maximum as the reasonable max
        (self.max_margin_usdc * 80) / 100
    }

    /// Calculate minimum and maximum reasonable liquidity for a given margin
    /// Based on the current working configuration and Uniswap V4 constraints
    pub fn calculate_liquidity_bounds(&self, margin_usdc: u128) -> (u128, u128) {
        // Calculate expected liquidity using Uniswap formula
        let expected_liquidity = self.calculate_liquidity_from_margin(margin_usdc);

        // For validation, allow a reasonable range
        // Minimum: 90% of expected (to account for rounding)
        let min_liquidity = (expected_liquidity * 9) / 10;

        // Maximum: 110% of expected (to account for rounding)
        let max_liquidity = (expected_liquidity * 11) / 10;

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

        // Test liquidity calculation for typical margins
        let test_margins = vec![10_000_000u128, 100_000_000u128, 1_000_000_000u128]; // 10, 100, 1000 USDC
        for margin in test_margins {
            let calculated_liq = self.calculate_liquidity_from_margin(margin);

            // Ensure liquidity is reasonable (non-zero)
            if calculated_liq == 0 {
                return Err(format!(
                    "{} USDC margin produces zero liquidity",
                    margin as f64 / 1_000_000.0
                ));
            }

            tracing::debug!(
                "{} USDC: liquidity {}",
                margin as f64 / 1_000_000.0,
                calculated_liq
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
            default_tick_lower: -46080, // Price 0.01 (matches OpenMakerPosition.sol)
            default_tick_upper: 46050,  // Price ~100 (matches OpenMakerPosition.sol)
            liquidity_scaling_factor: 200_000_000_000_000_000_000, // 200e18 (matches OpenMakerPosition.sol)
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
    pub multicall3_abi: JsonAbi,
    pub beacon_factory_address: Address,
    pub perpcity_registry_address: Address,
    pub perp_hook_address: Address,
    pub usdc_address: Address,
    pub usdc_transfer_limit: u128,
    pub eth_transfer_limit: u128,
    pub access_token: String,
    pub perp_config: PerpConfig,
    pub multicall3_address: Option<Address>, // Optional multicall3 contract for batch operations
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BeaconUpdateData {
    pub beacon_address: String,
    pub value: u64,
    pub proof: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchUpdateBeaconRequest {
    pub updates: Vec<BeaconUpdateData>,
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
    /// **IMPORTANT**: Due to Uniswap V4 liquidity requirements and the tick range [40950, 46050],
    /// minimum recommended amount is 10 USDC (10,000,000). Smaller amounts will likely fail
    /// with execution revert due to insufficient liquidity.
    ///
    /// Current scaling: margin × 100,000 = final liquidity amount
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
