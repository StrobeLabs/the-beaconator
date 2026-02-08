// Unit tests for perp validation service layer
use the_beaconator::services::perp::validation::{ContractErrorDecoder, try_decode_revert_reason};

#[cfg(test)]
mod contract_error_decoder_tests {
    use super::*;

    #[test]
    fn test_decode_opening_leverage_out_of_bounds() {
        // Selector + 3 uint256 params (64 hex chars each)
        let error_data = concat!(
            "0x239b350f",
            "0000000000000000000000000000000000000000000000000000000000000002",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "000000000000000000000000000000000000000000000000000000000000000a"
        );

        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("OpeningLeverageOutOfBounds"));
        assert!(message.contains("leverage"));
    }

    #[test]
    fn test_decode_opening_margin_out_of_bounds() {
        // Selector + 3 uint256 params
        let error_data = concat!(
            "0xcd4916f9",
            "0000000000000000000000000000000000000000000000000000000005f5e100",
            "0000000000000000000000000000000000000000000000000000000000989680",
            "000000000000000000000000000000000000000000000000000000003b9aca00"
        );

        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("OpeningMarginOutOfBounds"));
        assert!(message.contains("USDC"));
    }

    #[test]
    fn test_decode_invalid_liquidity() {
        // Selector + 1 uint256 param
        let error_data = concat!(
            "0x7e05cd27",
            "0000000000000000000000000000000000000000000000000000000000000000"
        );

        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("InvalidLiquidity"));
    }

    #[test]
    fn test_decode_live_position_details() {
        // Selector + 4 uint256 params
        let error_data = concat!(
            "0xd2aa461f",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "0000000000000000000000000000000000000000000000000000000000000002",
            "0000000000000000000000000000000000000000000000000000000000000003",
            "0000000000000000000000000000000000000000000000000000000000000000"
        );

        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("LivePositionDetails"));
    }

    #[test]
    fn test_decode_invalid_close() {
        // Selector + 3 params (2 addresses + 1 bool)
        let error_data = concat!(
            "0x2c328f64",
            "000000000000000000000000aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "000000000000000000000000bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "0000000000000000000000000000000000000000000000000000000000000001"
        );

        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("InvalidClose"));
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
    fn test_decode_unknown_custom_error_with_pool_address() {
        // Selector + 2 params
        let error_data = concat!(
            "0xfb8f41b2",
            "000000000000000000000000cccccccccccccccccccccccccccccccccccccccc",
            "0000000000000000000000000000000000000000000000000000000000000000"
        );

        let result = ContractErrorDecoder::decode_error_data(error_data);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("Unknown contract error"));
        assert!(message.contains("0xfb8f41b2"));
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
    fn test_decode_error_data_invalid_hex_in_params() {
        // Valid selector but params data too short (only 10 chars, needs 192)
        let error_data = "0x239b350fZZZZZZZZZZ";

        let result = ContractErrorDecoder::decode_error_data(error_data);
        // Should return None because params data is too short
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_opening_leverage_insufficient_params() {
        // Valid selector but insufficient parameter data
        let error_data = "0x239b350f00000000000000000000000000000000";

        let result = ContractErrorDecoder::decode_error_data(error_data);
        // Should return None because not enough data
        assert!(result.is_none());
    }
}

#[cfg(test)]
mod try_decode_revert_reason_tests {
    use super::*;

    #[test]
    fn test_decode_revert_with_custom_error() {
        // Error string containing custom error hex data
        let error = concat!(
            "execution reverted: 0x239b350f",
            "0000000000000000000000000000000000000000000000000000000000000002",
            "0000000000000000000000000000000000000000000000000000000000000001",
            "000000000000000000000000000000000000000000000000000000000000000a"
        );

        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("OpeningLeverageOutOfBounds"));
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
    fn test_decode_complex_error_message() {
        let error = concat!(
            "Error: transaction failed with status InternalServerError: execution reverted: 0x7e05cd27",
            "0000000000000000000000000000000000000000000000000000000000000000",
            " at block 12345"
        );

        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        let message = result.unwrap();
        assert!(message.contains("InvalidLiquidity"));
    }
}
