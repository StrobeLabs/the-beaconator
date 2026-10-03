// Unit tests for the perp validation / error decoder.
// Selectors come from `cast sig "<ErrorName>()"` against perpcity-contracts (build 58b42b7
// and tag v0.2.2-upgradeable).

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

    // ---- Solady SafeCastLib ----

    #[test]
    fn test_decode_safecast_overflow() {
        assert_contains("0x35278d12", "Overflow");
    }

    #[test]
    fn test_old_oz_safecast_selector_is_unknown() {
        let result = ContractErrorDecoder::decode_error_data("0x24775e06");
        assert!(result.unwrap().contains("Unknown contract error"));
    }

    // ---- v0.2.2-upgradeable additions ----

    #[test]
    fn test_decode_no_surplus() {
        assert_contains("0xc0ef17d3", "NoSurplus");
    }

    #[test]
    fn test_decode_zero_address() {
        assert_contains("0xd92e233d", "ZeroAddress");
    }

    #[test]
    fn test_decode_invalid_perp_implementation() {
        assert_contains("0xa457695f", "InvalidPerpImplementation");
    }

    #[test]
    fn test_decode_not_protocol_owner() {
        assert_contains("0xfb6fc0b7", "NotProtocolOwner");
    }

    #[test]
    fn test_decode_unauthorized_pool_action() {
        assert_contains("0xb7cc5070", "UnauthorizedPoolAction");
    }

    #[test]
    fn test_decode_token_does_not_exist() {
        assert_contains("0xceea21b6", "TokenDoesNotExist");
    }

    #[test]
    fn test_decode_erc1967_invalid_implementation_with_params() {
        let error_data = concat!(
            "0x4c9c8ce3",
            "0000000000000000000000001111111111111111111111111111111111111111"
        );
        assert_contains(error_data, "ERC1967InvalidImplementation");
    }

    #[test]
    fn test_decode_erc1967_non_payable() {
        assert_contains("0xb398979f", "ERC1967NonPayable");
    }

    #[test]
    fn test_decode_uups_unauthorized_call_context() {
        assert_contains("0xe07c8dba", "UUPSUnauthorizedCallContext");
    }

    #[test]
    fn test_decode_uups_unsupported_proxiable_uuid_with_params() {
        let error_data = concat!(
            "0xaa1d49a4",
            "360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc"
        );
        assert_contains(error_data, "UUPSUnsupportedProxiableUUID");
    }

    #[test]
    fn test_decode_invalid_initialization() {
        assert_contains("0xf92ee8a9", "InvalidInitialization");
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

    #[test]
    fn test_decode_revert_skips_address_picks_revert_data() {
        // Provider error message that includes an address (20-byte hex, 42 chars including 0x)
        // followed by the actual revert payload. The earlier implementation grabbed the first
        // 0x token and returned "Unknown contract error: 0x70997970..."; the fix must skip
        // the address and recognise the real selector.
        let error =
            "execution reverted at 0x70997970C51812dc3A010C7d01b50e0d17dc79C8, data: 0x10074548";
        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("ZeroLiquidity"), "got {msg}");
    }

    #[test]
    fn test_decode_revert_prefers_explicit_data_field() {
        // Alloy-style revert with explicit `data:` field — must be picked even if there are
        // other hex blobs in the message.
        let error = "ContractError(tx 0xabcdef1234567890, data: 0x38f5e1a7)";
        let result = try_decode_revert_reason(&error);
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("MarginTooLow"), "got {msg}");
    }
}
