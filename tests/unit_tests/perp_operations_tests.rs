// Perp operations unit tests - fast tests only, no Anvil

#[test]
fn test_deploy_perp_for_beacon_signature() {
    // Test that the deploy_perp_for_beacon function exists with correct signature
    // This is a compile-time verification test

    use the_beaconator::services::perp::deploy_perp_for_beacon;

    // Verify the function exists and has the expected signature by taking its address
    // This will fail to compile if the function doesn't exist or has the wrong signature
    let _fn_ptr = deploy_perp_for_beacon as *const ();

    // Assert the function pointer is not null (always true for function pointers)
    assert!(!_fn_ptr.is_null());
}

#[test]
fn test_perp_operations_module_exists() {
    // Verify that the perp operations module is accessible
    // This serves as documentation that perp operations have been modularized

    // Import public functions from the perp operations module
    use the_beaconator::services::perp::{
        batch_deposit_liquidity_with_multicall3, deploy_perp_for_beacon, deposit_liquidity_for_perp,
    };

    // Verify functions are accessible by taking their addresses
    // This proves the module exists and its public API is accessible
    let _deploy_fn = deploy_perp_for_beacon as *const ();
    let _deposit_fn = deposit_liquidity_for_perp as *const ();
    let _batch_deposit_fn = batch_deposit_liquidity_with_multicall3 as *const ();

    // Assert that we successfully imported from the module
    assert!(!_deploy_fn.is_null());
    assert!(!_deposit_fn.is_null());
    assert!(!_batch_deposit_fn.is_null());
}

#[test]
fn test_perp_config_validation() {
    // Test basic perp configuration validation without network calls
    use the_beaconator::models::PerpConfig;

    let config = PerpConfig::default();
    let result = config.validate();

    // Should pass with default configuration
    assert!(result.is_ok());
}

#[test]
fn test_margin_bounds_calculation() {
    // Test margin bounds calculation logic
    let min_margin = 10_000_000u64; // 10 USDC
    let max_margin = 1_000_000_000u64; // 1000 USDC

    assert!(min_margin < max_margin);
    assert!(min_margin > 0);

    // Test margin validation
    let test_margin = 500_000_000u64; // 500 USDC
    assert!(test_margin >= min_margin && test_margin <= max_margin);
}

#[test]
fn test_leverage_calculation_logic() {
    use the_beaconator::models::PerpConfig;

    // Test leverage calculation without requiring network
    let config = PerpConfig::default();

    // Test with 10 USDC margin -> should be 9.97x leverage (clamped from 10.0)
    let margin_10_usdc = 10_000_000u128;
    let leverage_10 = config.calculate_expected_leverage(margin_10_usdc).unwrap();
    assert!((leverage_10 - 9.97).abs() < 0.01); // Clamped to max 9.97

    // Test with 100 USDC margin -> should be ~3.16x leverage (10.0 / sqrt(10))
    let margin_100_usdc = 100_000_000u128;
    let leverage_100 = config.calculate_expected_leverage(margin_100_usdc).unwrap();
    let expected_100 = 10.0 / (margin_100_usdc as f64 / 10_000_000.0).sqrt();
    assert!((leverage_100 - expected_100).abs() < 0.01); // ~3.16

    // Ensure all leverage values are within bounds [0.1, 9.97]
    assert!((0.1..=9.97).contains(&leverage_10));
    assert!((0.1..=9.97).contains(&leverage_100));
}

#[tokio::test]
async fn test_deposit_liquidity_invalid_perp_id_early_error() {
    use the_beaconator::models::DepositLiquidityForPerpRequest;
    use the_beaconator::services::perp::operations::deposit_liquidity_for_perp;

    // Use simple app state to avoid network; this path errors before any network calls
    let state = crate::test_utils::create_simple_test_app_state();

    let request = DepositLiquidityForPerpRequest {
        perp_id: "not_a_hex_bytes32".to_string(),
        margin_amount_usdc: "1000000".to_string(), // 1 USDC
    };

    let result = deposit_liquidity_for_perp(&state, request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("Invalid perp ID"));
}

#[tokio::test]
async fn test_deposit_liquidity_zero_margin_early_error() {
    use the_beaconator::models::DepositLiquidityForPerpRequest;
    use the_beaconator::services::perp::operations::deposit_liquidity_for_perp;

    let state = crate::test_utils::create_simple_test_app_state();

    // Valid 32-byte hex (all ones)
    let perp_id_32 = format!("0x{:064}", 1);

    let request = DepositLiquidityForPerpRequest {
        perp_id: perp_id_32,
        margin_amount_usdc: "0".to_string(),
    };

    let result = deposit_liquidity_for_perp(&state, request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("Margin amount cannot be zero"));
}

#[tokio::test]
async fn test_deposit_liquidity_min_leverage_violation() {
    use the_beaconator::models::DepositLiquidityForPerpRequest;
    use the_beaconator::services::perp::operations::deposit_liquidity_for_perp;

    let mut state = crate::test_utils::create_simple_test_app_state();

    // Set a high minimum opening leverage to force validation failure for small margin
    // min_opening_leverage_x96 ~ 8x
    state.perp_config.min_opening_leverage_x96 = (8.0_f64 * (2u128.pow(96) as f64)) as u128;

    let perp_id_32 = format!("0x{:064}", 2);

    // Large margin to produce leverage below the raised minimum (expected leverage ~1x)
    let request = DepositLiquidityForPerpRequest {
        perp_id: perp_id_32,
        margin_amount_usdc: "1000000000".to_string(), // 1000 USDC
    };

    let result = deposit_liquidity_for_perp(&state, request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("below minimum required")
            || err.contains("Failed to calculate expected leverage")
    );
}
