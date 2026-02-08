use alloy::primitives::Address;
use alloy::providers::Provider;
use std::str::FromStr;
use std::sync::Arc;

use crate::ReadOnlyProvider;

/// Contract error decoding utilities for PerpManager errors
pub struct ContractErrorDecoder;

impl ContractErrorDecoder {
    // Known PerpManager error signatures
    const OPENING_LEVERAGE_OUT_OF_BOUNDS: &'static str = "0x239b350f";
    const OPENING_MARGIN_OUT_OF_BOUNDS: &'static str = "0xcd4916f9";
    const INVALID_LIQUIDITY: &'static str = "0x7e05cd27";
    const LIVE_POSITION_DETAILS: &'static str = "0xd2aa461f";
    const INVALID_CLOSE: &'static str = "0x2c328f64";
    const SAFECAST_OVERFLOW: &'static str = "0x24775e06";
    const UNKNOWN_CUSTOM_ERROR: &'static str = "0xfb8f41b2";

    pub fn decode_error_data(error_data: &str) -> Option<String> {
        if error_data.len() < 10 {
            return None;
        }

        let selector = &error_data[0..10];
        let params_data = &error_data[10..];

        match selector {
            Self::OPENING_LEVERAGE_OUT_OF_BOUNDS => {
                Self::decode_opening_leverage_out_of_bounds(params_data)
            }
            Self::OPENING_MARGIN_OUT_OF_BOUNDS => {
                Self::decode_opening_margin_out_of_bounds(params_data)
            }
            Self::INVALID_LIQUIDITY => Self::decode_invalid_liquidity(params_data),
            Self::LIVE_POSITION_DETAILS => Self::decode_live_position_details(params_data),
            Self::INVALID_CLOSE => Self::decode_invalid_close(params_data),
            Self::SAFECAST_OVERFLOW => Self::decode_safecast_overflow(params_data),
            Self::UNKNOWN_CUSTOM_ERROR => Self::decode_unknown_custom_error(params_data),
            _ => Some(format!("Unknown contract error: {selector}")),
        }
    }

    fn decode_opening_leverage_out_of_bounds(params_data: &str) -> Option<String> {
        if params_data.len() < 192 {
            // 3 * 64 hex chars
            return None;
        }

        // Parse the three uint parameters
        let leverage_x96_hex = &params_data[0..64];
        let min_leverage_x96_hex = &params_data[64..128];
        let max_leverage_x96_hex = &params_data[128..192];

        let leverage_x96 = u128::from_str_radix(leverage_x96_hex, 16).ok()?;
        let min_leverage_x96 = u128::from_str_radix(min_leverage_x96_hex, 16).ok()?;
        let max_leverage_x96 = u128::from_str_radix(max_leverage_x96_hex, 16).ok()?;

        // Convert X96 values to human readable
        let x96_factor = 2_u128.pow(96);
        let leverage = leverage_x96 as f64 / x96_factor as f64;
        let min_leverage = min_leverage_x96 as f64 / x96_factor as f64;
        let max_leverage = max_leverage_x96 as f64 / x96_factor as f64;

        Some(format!(
            "OpeningLeverageOutOfBounds: attempted {leverage:.2}x leverage, but must be between {min_leverage:.2}x and {max_leverage:.2}x"
        ))
    }

    fn decode_opening_margin_out_of_bounds(params_data: &str) -> Option<String> {
        if params_data.len() < 192 {
            // 3 * 64 hex chars
            return None;
        }

        let margin_hex = &params_data[0..64];
        let min_margin_hex = &params_data[64..128];
        let max_margin_hex = &params_data[128..192];

        let margin = u128::from_str_radix(margin_hex, 16).ok()?;
        let min_margin = u128::from_str_radix(min_margin_hex, 16).ok()?;
        let max_margin = u128::from_str_radix(max_margin_hex, 16).ok()?;

        // Convert to USDC (6 decimals)
        let margin_usdc = margin as f64 / 1_000_000.0;
        let min_margin_usdc = min_margin as f64 / 1_000_000.0;
        let max_margin_usdc = max_margin as f64 / 1_000_000.0;

        Some(format!(
            "OpeningMarginOutOfBounds: attempted {margin_usdc:.2} USDC margin, but must be between {min_margin_usdc:.2} and {max_margin_usdc:.2} USDC"
        ))
    }

    fn decode_invalid_liquidity(params_data: &str) -> Option<String> {
        if params_data.len() < 64 {
            return None;
        }

        let liquidity_hex = &params_data[0..64];
        let liquidity = u128::from_str_radix(liquidity_hex, 16).ok()?;

        Some(format!(
            "InvalidLiquidity: liquidity amount {liquidity} is invalid (must be > 0)"
        ))
    }

    fn decode_live_position_details(params_data: &str) -> Option<String> {
        if params_data.len() < 256 {
            // 4 * 64 hex chars
            return None;
        }

        // LivePositionDetails(int256 pnl, int256 funding, int256 effectiveMargin, bool isLiquidatable)
        Some("LivePositionDetails: Position details provided for liquidation analysis".to_string())
    }

    fn decode_invalid_close(params_data: &str) -> Option<String> {
        if params_data.len() < 192 {
            // 3 * 64 hex chars (2 addresses + bool)
            return None;
        }

        // InvalidClose(address caller, address holder, bool isLiquidated)
        Some("InvalidClose: Invalid attempt to close position".to_string())
    }

    fn decode_safecast_overflow(params_data: &str) -> Option<String> {
        if params_data.len() < 64 {
            return None;
        }

        let value_hex = &params_data[0..64];
        let value = u128::from_str_radix(value_hex, 16).ok()?;

        Some(format!(
            "SafeCastOverflowedUintToInt: value {value} overflows when casting to int"
        ))
    }

    fn decode_unknown_custom_error(params_data: &str) -> Option<String> {
        // Try to decode parameters if present
        if params_data.len() >= 128 {
            // Two parameters: address and uint256
            let pool_id_hex = &params_data[0..64];
            let param2_hex = &params_data[64..128];

            if let Ok(pool_address) = Address::from_str(&format!("0x{}", &pool_id_hex[24..])) {
                let param2_value = u128::from_str_radix(param2_hex, 16).unwrap_or(0);
                Some(format!(
                    "Unknown contract error (0xfb8f41b2) - pool: {pool_address}, value: {param2_value}. This error signature is not recognized in the PerpManager contract."
                ))
            } else {
                Some("Unknown contract error (0xfb8f41b2) with parameters. Check contract logs for details.".to_string())
            }
        } else if params_data.len() >= 64 {
            // Single address parameter
            let pool_id_hex = &params_data[0..64];
            if let Ok(pool_address) = Address::from_str(&format!("0x{}", &pool_id_hex[24..])) {
                Some(format!(
                    "Unknown contract error (0xfb8f41b2) with pool address: {pool_address}. Check contract logs for details."
                ))
            } else {
                Some("Unknown contract error (0xfb8f41b2) with parameters. Check contract logs for details.".to_string())
            }
        } else {
            Some(
                "Unknown contract error (0xfb8f41b2). Check contract logs for details.".to_string(),
            )
        }
    }
}

/// Helper function to validate that a module address has deployed code
pub async fn validate_module_address(
    provider: &Arc<ReadOnlyProvider>,
    address: Address,
    module_name: &str,
) -> Result<(), String> {
    match provider.get_code_at(address).await {
        Ok(code) => {
            if code.is_empty() {
                let error_msg = format!(
                    "{module_name} address {address} has no deployed code (not a contract)"
                );
                tracing::error!("{}", error_msg);
                Err(error_msg)
            } else {
                tracing::info!(
                    "{} address {} validated ({} bytes of code)",
                    module_name,
                    address,
                    code.len()
                );
                Ok(())
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to validate {module_name} address {address}: {e}");
            tracing::error!("{}", error_msg);
            Err(error_msg)
        }
    }
}

/// Helper function to try to decode revert reason from error
pub fn try_decode_revert_reason(error: &impl std::fmt::Display) -> Option<String> {
    let error_str = error.to_string();

    // Look for hex data in the error message
    if let Some(data_start) = error_str.find("0x") {
        let data_part = &error_str[data_start..];
        // Extract just the hex part (stop at first non-hex character after 0x)
        let hex_end = data_part
            .chars()
            .skip(2) // Skip "0x"
            .take_while(|c| c.is_ascii_hexdigit())
            .count()
            + 2;

        if hex_end > 10 {
            // At least selector + some data
            let error_data = &data_part[..hex_end];
            if let Some(decoded) = ContractErrorDecoder::decode_error_data(error_data) {
                return Some(decoded);
            }
        }
    }

    // Fallback to original logic
    if error_str.contains("execution reverted") {
        if let Some(reason) = error_str.split("execution reverted").nth(1) {
            let cleaned = reason.trim().trim_matches('"').trim_matches(':').trim();
            if !cleaned.is_empty() {
                return Some(format!("Revert reason: {cleaned}"));
            }
        }
        return Some("Execution reverted (no specific reason provided)".to_string());
    }

    None
}
