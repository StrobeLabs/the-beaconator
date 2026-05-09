use alloy::primitives::Address;
use alloy::providers::Provider;
use std::sync::Arc;

use crate::ReadOnlyProvider;

/// Decodes 4-byte error selectors emitted by perpcity-contracts@v0.1.0 (`Perp.sol`,
/// `PerpFactory.sol`, `ProtocolFeeManager.sol`) into human-readable strings for API responses.
///
/// Selectors are derived from the v0.1.0 contracts via `cast sig "<ErrorName>()"` (and similar
/// for parameterized errors). Update this list whenever the pinned contracts version bumps.
pub struct ContractErrorDecoder;

impl ContractErrorDecoder {
    // From src/libraries/Errors.sol@v0.1.0 — all parameterless.
    const ZERO_DELTA: &'static str = "0x6f0f5899";
    const MIN_AMT_UNMET: &'static str = "0x0470009e";
    const MARGIN_TOO_LOW: &'static str = "0x38f5e1a7";
    const NO_SYSTEM_FUNDS: &'static str = "0x5c64c19c";
    const ZERO_LIQUIDITY: &'static str = "0x10074548";
    const MAX_AMT_EXCEEDED: &'static str = "0x24f14ba6";
    const NEGATIVE_EQUITY: &'static str = "0xfece0035";
    const NEGATIVE_MARGIN: &'static str = "0xe94943ae";
    const NOT_POOL_MANAGER: &'static str = "0xae18210a";
    const NOT_LIQUIDATABLE: &'static str = "0xddeb79ba";
    const NON_MAKER_POSITION: &'static str = "0xdbcefbf3";
    const NON_TAKER_POSITION: &'static str = "0x12d39e8a";
    const TICKS_OUT_OF_BOUNDS: &'static str = "0xd6acf910";
    const MARGIN_RATIO_TOO_LOW: &'static str = "0xb2c649db";
    const PRICE_IMPACT_TOO_HIGH: &'static str = "0xfb30d03a";
    const UNAUTHORIZED_CALLER: &'static str = "0x5c427cd9";
    const POSITION_DOES_NOT_EXIST: &'static str = "0xf7b3b391";
    const LONG_UTILIZATION_EXCEEDED: &'static str = "0xcefb0b13";
    const SHORT_UTILIZATION_EXCEEDED: &'static str = "0x3615a2a2";
    const INSUFFICIENT_LIQUIDITY_TO_FILL: &'static str = "0xed126f97";
    const DATA_ALREADY_PENDING: &'static str = "0xd91ff208";
    const DATA_NOT_TIMELOCKED: &'static str = "0x1ea942a8";
    const TIMELOCK_NOT_EXPIRED: &'static str = "0x621e25c3";
    const ABDICATED: &'static str = "0x281df4aa";

    // From src/interfaces/IPerpFactory.sol@v0.1.0.
    const STARTING_PRICE_TOO_LOW: &'static str = "0xac8ac5a5";
    const STARTING_PRICE_TOO_HIGH: &'static str = "0x32231715";
    const EMA_WINDOW_TOO_LOW: &'static str = "0xc657a809";

    // From src/interfaces/IProtocolFeeManager.sol@v0.1.0.
    const PROTOCOL_FEE_TOO_HIGH: &'static str = "0x499fddb1";

    // Solady SafeCastLib — has parameter (the offending uint).
    const SAFECAST_OVERFLOW: &'static str = "0x24775e06";

    pub fn decode_error_data(error_data: &str) -> Option<String> {
        if error_data.len() < 10 {
            return None;
        }

        let selector = &error_data[0..10];
        let params_data = &error_data[10..];

        match selector {
            Self::ZERO_DELTA => Some("ZeroDelta: requested perp delta is zero".to_string()),
            Self::MIN_AMT_UNMET => {
                Some("MinAmtUnmet: swap result fell short of the requested minimum".to_string())
            }
            Self::MARGIN_TOO_LOW => {
                Some("MarginTooLow: margin is below the module's minimum".to_string())
            }
            Self::NO_SYSTEM_FUNDS => {
                Some("NoSystemFunds: nothing collectable from system fee accumulators".to_string())
            }
            Self::ZERO_LIQUIDITY => {
                Some("ZeroLiquidity: liquidity must be greater than zero".to_string())
            }
            Self::MAX_AMT_EXCEEDED => {
                Some("MaxAmtExceeded: deposit/withdraw exceeded the requested max".to_string())
            }
            Self::NEGATIVE_EQUITY => {
                Some("NegativeEquity: position equity is negative".to_string())
            }
            Self::NEGATIVE_MARGIN => {
                Some("NegativeMargin: resulting margin is negative".to_string())
            }
            Self::NOT_POOL_MANAGER => {
                Some("NotPoolManager: caller is not the Uniswap V4 PoolManager".to_string())
            }
            Self::NOT_LIQUIDATABLE => {
                Some("NotLiquidatable: position is not below liquidation threshold".to_string())
            }
            Self::NON_MAKER_POSITION => {
                Some("NonMakerPosition: position is not a maker position".to_string())
            }
            Self::NON_TAKER_POSITION => {
                Some("NonTakerPosition: position is not a taker position".to_string())
            }
            Self::TICKS_OUT_OF_BOUNDS => {
                Some("TicksOutOfBounds: tick range is outside valid bounds".to_string())
            }
            Self::MARGIN_RATIO_TOO_LOW => {
                Some("MarginRatioTooLow: margin ratio is below the initial threshold".to_string())
            }
            Self::PRICE_IMPACT_TOO_HIGH => {
                Some("PriceImpactTooHigh: swap exceeds the PriceImpact module's bounds".to_string())
            }
            Self::UNAUTHORIZED_CALLER => {
                Some("UnauthorizedCaller: caller is not authorized for this position".to_string())
            }
            Self::POSITION_DOES_NOT_EXIST => {
                Some("PositionDoesNotExist: the specified position id does not exist".to_string())
            }
            Self::LONG_UTILIZATION_EXCEEDED => Some(
                "LongUtilizationExceeded: long open interest exceeds available capacity"
                    .to_string(),
            ),
            Self::SHORT_UTILIZATION_EXCEEDED => Some(
                "ShortUtilizationExceeded: short open interest exceeds available capacity"
                    .to_string(),
            ),
            Self::INSUFFICIENT_LIQUIDITY_TO_FILL => Some(
                "InsufficientLiquidityToFill: AMM has insufficient liquidity for this trade"
                    .to_string(),
            ),
            Self::DATA_ALREADY_PENDING => {
                Some("DataAlreadyPending: a timelocked update is already pending".to_string())
            }
            Self::DATA_NOT_TIMELOCKED => {
                Some("DataNotTimelocked: no pending timelocked update for this data".to_string())
            }
            Self::TIMELOCK_NOT_EXPIRED => {
                Some("TimelockNotExpired: timelock period has not yet elapsed".to_string())
            }
            Self::ABDICATED => {
                Some("Abdicated: this admin function has been permanently abdicated".to_string())
            }
            Self::STARTING_PRICE_TOO_LOW => Some(
                "StartingPriceTooLow: beacon index implies a sqrt price below the AMM minimum"
                    .to_string(),
            ),
            Self::STARTING_PRICE_TOO_HIGH => Some(
                "StartingPriceTooHigh: beacon index implies a sqrt price above the AMM maximum"
                    .to_string(),
            ),
            Self::EMA_WINDOW_TOO_LOW => {
                Some("EmaWindowTooLow: emaWindow must be > 0 (uint24)".to_string())
            }
            Self::PROTOCOL_FEE_TOO_HIGH => Some(
                "ProtocolFeeTooHigh: requested protocol fee exceeds the configured maximum"
                    .to_string(),
            ),
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

/// Validates that a module address has deployed bytecode (i.e. is actually a contract).
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

/// Best-effort revert-reason decoder: looks for hex-encoded revert data in an error string and
/// dispatches to `ContractErrorDecoder`. Falls back to plain "execution reverted" extraction.
pub fn try_decode_revert_reason(error: &impl std::fmt::Display) -> Option<String> {
    let error_str = error.to_string();

    if let Some(data_start) = error_str.find("0x") {
        let data_part = &error_str[data_start..];
        let hex_end = data_part
            .chars()
            .skip(2)
            .take_while(|c| c.is_ascii_hexdigit())
            .count()
            + 2;

        if hex_end >= 10 {
            let error_data = &data_part[..hex_end];
            if let Some(decoded) = ContractErrorDecoder::decode_error_data(error_data) {
                return Some(decoded);
            }
        }
    }

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
