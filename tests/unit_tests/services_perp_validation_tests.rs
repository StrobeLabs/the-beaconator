// Unit tests for the v0.1.0 perp validation / error decoder.
// Selectors come from `cast sig "<ErrorName>()"` against perpcity-contracts@v0.1.0.

use the_beaconator::services::perp::validation::{ContractErrorDecoder, try_decode_revert_reason};

#[cfg(test)]
mod contract_error_decoder_tests {
    use super::*;

    fn assert_contains(selector: &str, expected_substring: &str) {
        let result = ContractErrorDecoder::decode_error_data(selector);
        assert!(
            result.is_some(),
            "expected decode for {selector} to be Some"
        );
        let msg = result.unwrap();
        assert!(
            msg.contains(expected_substring),
            "selector {selector} decoded to {msg:?}, expected substring {expected_substring:?}"
        );
    }

    // ---- src/libraries/Errors.sol@v0.1.0 ----

    #[test]
    fn test_decode_zero_delta() {
        assert_contains("0x6f0f5899", "ZeroDelta");
    }

    #[test]
    fn test_decode_min_amt_unmet() {
        assert_contains("0x0470009e", "MinAmtUnmet");
    }

    #[test]
    fn test_decode_margin_too_low() {
        assert_contains("0x38f5e1a7", "MarginTooLow");
    }

    #[test]
    fn test_decode_no_system_funds() {
        assert_contains("0x5c64c19c", "NoSystemFunds");
    }

    #[test]
    fn test_decode_zero_liquidity() {
        assert_contains("0x10074548", "ZeroLiquidity");
    }

    #[test]
    fn test_decode_max_amt_exceeded() {
        assert_contains("0x24f14ba6", "MaxAmtExceeded");
    }

    #[test]
    fn test_decode_negative_equity() {
        assert_contains("0xfece0035", "NegativeEquity");
    }

    #[test]
    fn test_decode_negative_margin() {
        assert_contains("0xe94943ae", "NegativeMargin");
    }

    #[test]
    fn test_decode_not_pool_manager() {
        assert_contains("0xae18210a", "NotPoolManager");
    }

    #[test]
    fn test_decode_not_liquidatable() {
        assert_contains("0xddeb79ba", "NotLiquidatable");
    }

    #[test]
    fn test_decode_non_maker_position() {
        assert_contains("0xdbcefbf3", "NonMakerPosition");
    }

    #[test]
    fn test_decode_non_taker_position() {
        assert_contains("0x12d39e8a", "NonTakerPosition");
    }

    #[test]
    fn test_decode_ticks_out_of_bounds() {
        assert_contains("0xd6acf910", "TicksOutOfBounds");
    }

    #[test]
    fn test_decode_margin_ratio_too_low() {
        assert_contains("0xb2c649db", "MarginRatioTooLow");
    }

    #[test]
    fn test_decode_price_impact_too_high() {
        assert_contains("0xfb30d03a", "PriceImpactTooHigh");
    }

    #[test]
    fn test_decode_unauthorized_caller() {
        assert_contains("0x5c427cd9", "UnauthorizedCaller");
    }

    #[test]
    fn test_decode_position_does_not_exist() {
        assert_contains("0xf7b3b391", "PositionDoesNotExist");
    }

    #[test]
    fn test_decode_long_utilization_exceeded() {
        assert_contains("0xcefb0b13", "LongUtilizationExceeded");
    }

    #[test]
    fn test_decode_short_utilization_exceeded() {
        assert_contains("0x3615a2a2", "ShortUtilizationExceeded");
    }

    #[test]
    fn test_decode_insufficient_liquidity_to_fill() {
        assert_contains("0xed126f97", "InsufficientLiquidityToFill");
    }

    #[test]
    fn test_decode_data_already_pending() {
        assert_contains("0xd91ff208", "DataAlreadyPending");
    }

    #[test]
    fn test_decode_data_not_timelocked() {
        assert_contains("0x1ea942a8", "DataNotTimelocked");
    }

    #[test]
    fn test_decode_timelock_not_expired() {
        assert_contains("0x621e25c3", "TimelockNotExpired");
    }

    #[test]
    fn test_decode_abdicated() {
        assert_contains("0x281df4aa", "Abdicated");
    }

    // ---- src/interfaces/IPerpFactory.sol@v0.1.0 ----

    #[test]
    fn test_decode_starting_price_too_low() {
        assert_contains("0xac8ac5a5", "StartingPriceTooLow");
    }

    #[test]
    fn test_decode_starting_price_too_high() {
        assert_contains("0x32231715", "StartingPriceTooHigh");
    }

    #[test]
    fn test_decode_ema_window_too_low() {
        assert_contains("0xc657a809", "EmaWindowTooLow");
    }

    // ---- src/interfaces/IProtocolFeeManager.sol@v0.1.0 ----

    #[test]
    fn test_decode_protocol_fee_too_high() {
        assert_contains("0x499fddb1", "ProtocolFeeTooHigh");
    }

    // ---- Solady SafeCastLib (parameterized) ----

    #[test]
    fn test_decode_safecast_overflow() {
        let error_data = concat!(
            "0x24775e06",
            "00000000000000000000000000000000ffffffffffffffffffffffffffffffff"
        );
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("SafeCastOverflowedUintToInt"));
    }

    // ---- Edge cases ----

    #[test]
    fn test_decode_unknown_selector() {
        let error_data = concat!(
            "0xdeadbeef",
            "0000000000000000000000000000000000000000000000000000000000000000"
        );
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("Unknown contract error"));
        assert!(message.contains("0xdeadbeef"));
    }

    #[test]
    fn test_decode_error_data_too_short() {
        let error_data = "0x1234";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_safecast_overflow_insufficient_params() {
        let error_data = "0x24775e0600000000000000000000000000000000";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_none());
    }

    #[test]
    fn test_parameterless_errors_work_with_trailing_data() {
        let error_data = concat!(
            "0x10074548",
            "0000000000000000000000000000000000000000000000000000000000000000"
        );
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("ZeroLiquidity"));
    }
}

#[cfg(test)]
mod try_decode_revert_reason_tests {
    use super::*;

    #[test]
    fn test_decode_revert_with_custom_error() {
        let error = "execution reverted: 0x10074548";
        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        assert!(result.unwrap().contains("ZeroLiquidity"));
    }

    #[test]
    fn test_decode_revert_with_string_reason() {
        let error = "execution reverted: insufficient balance";
        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        assert!(
            result
                .unwrap()
                .contains("Revert reason: insufficient balance")
        );
    }

    #[test]
    fn test_decode_revert_no_reason() {
        let error = "execution reverted";
        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        assert!(
            result
                .unwrap()
                .contains("Execution reverted (no specific reason provided)")
        );
    }

    #[test]
    fn test_decode_non_revert_error() {
        let error = "network timeout error";
        let result = try_decode_revert_reason(&error);
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_revert_with_short_hex() {
        let error = "execution reverted: 0x1234";
        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("Revert reason") || message.contains("Execution reverted"));
    }

    #[test]
    fn test_decode_revert_with_unknown_selector() {
        let error = concat!(
            "execution reverted: 0xdeadbeef",
            "0000000000000000000000000000000000000000000000000000000000000000"
        );
        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        assert!(
            result
                .unwrap()
                .contains("Unknown contract error: 0xdeadbeef")
        );
    }

    #[test]
    fn test_decode_revert_quoted_reason() {
        let error = "execution reverted: \"custom error message\"";
        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        assert!(result.unwrap().contains("custom error message"));
    }

    #[test]
    fn test_decode_revert_with_margin_too_low() {
        // Verifies the new (v0.1.0) MarginTooLow error decodes via the full revert pipeline.
        let error = "execution reverted: 0x38f5e1a7";
        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        assert!(result.unwrap().contains("MarginTooLow"));
    }
}
