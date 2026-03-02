use alloy::primitives::Address;
use alloy::providers::Provider;
use std::sync::Arc;

use crate::ReadOnlyProvider;

/// Contract error decoding utilities for PerpManager errors
///
/// All new PerpManager errors are parameterless, making decoding straightforward.
pub struct ContractErrorDecoder;

impl ContractErrorDecoder {
    // Known PerpManager error selectors (all parameterless)
    const ZERO_LIQUIDITY: &'static str = "0x10074548";
    const ZERO_NOTIONAL: &'static str = "0x96bafbfd";
    const TICKS_OUT_OF_BOUNDS: &'static str = "0xd6acf910";
    const INVALID_MARGIN: &'static str = "0x3a29e65e";
    const INVALID_MARGIN_DELTA: &'static str = "0x8acc6d7f";
    const INVALID_CALLER: &'static str = "0x48f5c3ed";
    const POSITION_LOCKED: &'static str = "0xc7d26d72";
    const ZERO_DELTA: &'static str = "0x6f0f5899";
    const INVALID_MARGIN_RATIO: &'static str = "0xbcffc83f";
    const FEES_NOT_REGISTERED: &'static str = "0x2872ed04";
    const MARGIN_RATIOS_NOT_REGISTERED: &'static str = "0x3eea589d";
    const LOCKUP_PERIOD_NOT_REGISTERED: &'static str = "0xd9f0aeaf";
    const SQRT_PRICE_IMPACT_LIMIT_NOT_REGISTERED: &'static str = "0x5140209c";
    const FEE_TOO_LARGE: &'static str = "0xfc5bee12";
    const MAKER_NOT_ALLOWED: &'static str = "0xc3f6bb4e";
    const BEACON_NOT_REGISTERED: &'static str = "0x7884e2a9";
    const PERP_DOES_NOT_EXIST: &'static str = "0x232ad152";
    const STARTING_SQRT_PRICE_TOO_LOW: &'static str = "0x1d8648bc";
    const STARTING_SQRT_PRICE_TOO_HIGH: &'static str = "0x0947cb52";
    const COULD_NOT_FULLY_FILL: &'static str = "0x67cf2eaa";

    // Solady SafeCast error (still relevant, has parameters)
    const SAFECAST_OVERFLOW: &'static str = "0x24775e06";

    pub fn decode_error_data(error_data: &str) -> Option<String> {
        if error_data.len() < 10 {
            return None;
        }

        let selector = &error_data[0..10];
        let params_data = &error_data[10..];

        match selector {
            Self::ZERO_LIQUIDITY => Some("ZeroLiquidity: liquidity must be greater than zero".to_string()),
            Self::ZERO_NOTIONAL => Some("ZeroNotional: notional value must be greater than zero".to_string()),
            Self::TICKS_OUT_OF_BOUNDS => Some("TicksOutOfBounds: tick range is outside valid bounds".to_string()),
            Self::INVALID_MARGIN => Some("InvalidMargin: margin amount is invalid".to_string()),
            Self::INVALID_MARGIN_DELTA => Some("InvalidMarginDelta: margin delta is invalid".to_string()),
            Self::INVALID_CALLER => Some("InvalidCaller: caller is not authorized".to_string()),
            Self::POSITION_LOCKED => Some("PositionLocked: position is still within lockup period".to_string()),
            Self::ZERO_DELTA => Some("ZeroDelta: delta must be non-zero".to_string()),
            Self::INVALID_MARGIN_RATIO => Some("InvalidMarginRatio: margin ratio is invalid".to_string()),
            Self::FEES_NOT_REGISTERED => Some("FeesNotRegistered: fees module is not registered with PerpManager".to_string()),
            Self::MARGIN_RATIOS_NOT_REGISTERED => Some("MarginRatiosNotRegistered: margin ratios module is not registered with PerpManager".to_string()),
            Self::LOCKUP_PERIOD_NOT_REGISTERED => Some("LockupPeriodNotRegistered: lockup period module is not registered with PerpManager".to_string()),
            Self::SQRT_PRICE_IMPACT_LIMIT_NOT_REGISTERED => Some("SqrtPriceImpactLimitNotRegistered: sqrt price impact limit module is not registered with PerpManager".to_string()),
            Self::FEE_TOO_LARGE => Some("FeeTooLarge: fee exceeds maximum allowed value".to_string()),
            Self::MAKER_NOT_ALLOWED => Some("MakerNotAllowed: maker positions are not allowed for this perp".to_string()),
            Self::BEACON_NOT_REGISTERED => Some("BeaconNotRegistered: beacon is not registered with the registry".to_string()),
            Self::PERP_DOES_NOT_EXIST => Some("PerpDoesNotExist: the specified perp ID does not exist".to_string()),
            Self::STARTING_SQRT_PRICE_TOO_LOW => Some("StartingSqrtPriceTooLow: starting sqrt price is below minimum".to_string()),
            Self::STARTING_SQRT_PRICE_TOO_HIGH => Some("StartingSqrtPriceTooHigh: starting sqrt price exceeds maximum".to_string()),
            Self::COULD_NOT_FULLY_FILL => Some("CouldNotFullyFill: order could not be fully filled".to_string()),
            Self::SAFECAST_OVERFLOW => Self::decode_safecast_overflow(params_data),
            _ => Some(format!("Unknown contract error: {selector}")),
        }
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

        if hex_end >= 10 {
            // At least a full selector (parameterless errors are exactly 10 chars)
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
