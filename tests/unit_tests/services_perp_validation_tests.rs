// Unit tests for perp validation service layer
use the_beaconator::services::perp::validation::{ContractErrorDecoder, try_decode_revert_reason};

#[cfg(test)]
mod contract_error_decoder_tests {
    use super::*;

    #[test]
    fn test_decode_zero_liquidity() {
        let error_data = "0x10074548";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("ZeroLiquidity"));
    }

    #[test]
    fn test_decode_zero_notional() {
        let error_data = "0x96bafbfd";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("ZeroNotional"));
    }

    #[test]
    fn test_decode_ticks_out_of_bounds() {
        let error_data = "0xd6acf910";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("TicksOutOfBounds"));
    }

    #[test]
    fn test_decode_invalid_margin() {
        let error_data = "0x3a29e65e";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("InvalidMargin"));
    }

    #[test]
    fn test_decode_invalid_margin_delta() {
        let error_data = "0x8acc6d7f";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("InvalidMarginDelta"));
    }

    #[test]
    fn test_decode_invalid_caller() {
        let error_data = "0x48f5c3ed";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("InvalidCaller"));
    }

    #[test]
    fn test_decode_position_locked() {
        let error_data = "0xc7d26d72";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("PositionLocked"));
    }

    #[test]
    fn test_decode_zero_delta() {
        let error_data = "0x6f0f5899";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("ZeroDelta"));
    }

    #[test]
    fn test_decode_invalid_margin_ratio() {
        let error_data = "0xbcffc83f";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("InvalidMarginRatio"));
    }

    #[test]
    fn test_decode_fees_not_registered() {
        let error_data = "0x2872ed04";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("FeesNotRegistered"));
    }

    #[test]
    fn test_decode_margin_ratios_not_registered() {
        let error_data = "0x3eea589d";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("MarginRatiosNotRegistered"));
    }

    #[test]
    fn test_decode_lockup_period_not_registered() {
        let error_data = "0xd9f0aeaf";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("LockupPeriodNotRegistered"));
    }

    #[test]
    fn test_decode_sqrt_price_impact_limit_not_registered() {
        let error_data = "0x5140209c";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(
            result
                .unwrap()
                .contains("SqrtPriceImpactLimitNotRegistered")
        );
    }

    #[test]
    fn test_decode_fee_too_large() {
        let error_data = "0xfc5bee12";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("FeeTooLarge"));
    }

    #[test]
    fn test_decode_maker_not_allowed() {
        let error_data = "0xc3f6bb4e";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("MakerNotAllowed"));
    }

    #[test]
    fn test_decode_beacon_not_registered() {
        let error_data = "0x7884e2a9";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("BeaconNotRegistered"));
    }

    #[test]
    fn test_decode_perp_does_not_exist() {
        let error_data = "0x232ad152";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("PerpDoesNotExist"));
    }

    #[test]
    fn test_decode_starting_sqrt_price_too_low() {
        let error_data = "0x1d8648bc";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("StartingSqrtPriceTooLow"));
    }

    #[test]
    fn test_decode_starting_sqrt_price_too_high() {
        let error_data = "0x0947cb52";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("StartingSqrtPriceTooHigh"));
    }

    #[test]
    fn test_decode_could_not_fully_fill() {
        let error_data = "0x67cf2eaa";
        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        assert!(result.unwrap().contains("CouldNotFullyFill"));
    }

    #[test]
    fn test_decode_safecast_overflow() {
        // Selector + 1 uint256 param (using a value that fits in u128)
        let error_data = concat!(
            "0x24775e06",
            "00000000000000000000000000000000ffffffffffffffffffffffffffffffff"
        );

        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("SafeCastOverflowedUintToInt"));
    }

    #[test]
    fn test_decode_unknown_selector() {
        // Unrecognized selector
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
        // Error data too short (less than 10 chars for selector)
        let error_data = "0x1234";

        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_safecast_overflow_insufficient_params() {
        // Valid selector but insufficient parameter data
        let error_data = "0x24775e0600000000000000000000000000000000";

        let result = ContractErrorDecoder::decode_error_data(error_data);
        // Should return None because not enough data for SafeCast params
        assert!(result.is_none());
    }

    #[test]
    fn test_parameterless_errors_work_with_trailing_data() {
        // Parameterless errors should still decode even with extra data appended
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
        // Error string containing a new PerpManager error selector
        let error = "execution reverted: 0x10074548";

        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("ZeroLiquidity"));
    }

    #[test]
    fn test_decode_revert_with_string_reason() {
        let error = "execution reverted: insufficient balance";

        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("Revert reason: insufficient balance"));
    }

    #[test]
    fn test_decode_revert_no_reason() {
        let error = "execution reverted";

        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("Execution reverted (no specific reason provided)"));
    }

    #[test]
    fn test_decode_non_revert_error() {
        let error = "network timeout error";

        let result = try_decode_revert_reason(&error);
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_revert_with_short_hex() {
        // Hex data too short to be a custom error (less than selector size)
        let error = "execution reverted: 0x1234";

        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        let message = result.unwrap();
        // Should fall back to the string reason since hex is too short
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
        let message = result.unwrap();
        assert!(message.contains("Unknown contract error: 0xdeadbeef"));
    }

    #[test]
    fn test_decode_revert_quoted_reason() {
        let error = "execution reverted: \"custom error message\"";

        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("custom error message"));
    }

    #[test]
    fn test_decode_revert_with_beacon_not_registered() {
        let error = "execution reverted: 0x7884e2a9";

        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("BeaconNotRegistered"));
    }
}
