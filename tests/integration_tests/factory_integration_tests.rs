// Integration tests for factory beacon creation services

use alloy::primitives::Address;
use std::str::FromStr;
use the_beaconator::models::beacon_type::{BeaconTypeConfig, FactoryType};
use the_beaconator::models::requests::{
    CreateLBCGBMBeaconRequest, CreateWeightedSumCompositeBeaconRequest,
};
use the_beaconator::services::beacon::factory::{
    create_lbcgbm_beacon, create_weighted_sum_composite_beacon,
};

fn make_lbcgbm_config() -> BeaconTypeConfig {
    BeaconTypeConfig {
        slug: "lbcgbm".to_string(),
        name: "LBCGBM".to_string(),
        description: None,
        factory_address: Address::from_str("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
        factory_type: FactoryType::LBCGBM,
        registry_address: None,
        enabled: true,
        created_at: 0,
        updated_at: 0,
    }
}

fn make_composite_config() -> BeaconTypeConfig {
    BeaconTypeConfig {
        slug: "weighted-sum-composite".to_string(),
        name: "WeightedSumComposite".to_string(),
        description: None,
        factory_address: Address::from_str("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap(),
        factory_type: FactoryType::WeightedSumComposite,
        registry_address: None,
        enabled: true,
        created_at: 0,
        updated_at: 0,
    }
}

fn make_lbcgbm_request() -> CreateLBCGBMBeaconRequest {
    CreateLBCGBMBeaconRequest {
        measurement_scale: 1_000_000_000_000_000_000,
        sigma_base: 50_000_000_000_000_000,
        scaling_factor: 1_000_000_000_000_000_000,
        alpha: 500_000_000_000_000_000,
        decay: 950_000_000_000_000_000,
        initial_sigma_ratio: 1_000_000_000_000_000_000,
        variance_scaling: true,
        min_index: 100_000_000_000_000_000_000,
        max_index: 10_000_000_000_000_000_000_000,
        steepness: 1_000_000_000_000_000_000,
        initial_index: 1_000_000_000_000_000_000_000,
    }
}

// ============================================================================
// COMPOSITE BEACON VALIDATION TESTS
// ============================================================================

#[tokio::test]
async fn test_composite_beacon_mismatched_lengths_fails() {
    let app_state = crate::test_utils::create_simple_test_app_state().await;
    let config = make_composite_config();

    let request = CreateWeightedSumCompositeBeaconRequest {
        reference_beacons: vec![
            "0x1111111111111111111111111111111111111111".to_string(),
            "0x2222222222222222222222222222222222222222".to_string(),
        ],
        weights: vec![1_000_000_000_000_000_000], // Only 1 weight for 2 beacons
    };

    let result = create_weighted_sum_composite_beacon(&app_state, &config, &request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("must match"),
        "Expected length mismatch error, got: {err}"
    );
}

#[tokio::test]
async fn test_composite_beacon_empty_arrays_fails() {
    let app_state = crate::test_utils::create_simple_test_app_state().await;
    let config = make_composite_config();

    let request = CreateWeightedSumCompositeBeaconRequest {
        reference_beacons: vec![],
        weights: vec![],
    };

    let result = create_weighted_sum_composite_beacon(&app_state, &config, &request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("must not be empty"),
        "Expected empty error, got: {err}"
    );
}

#[tokio::test]
async fn test_composite_beacon_invalid_address_fails() {
    let app_state = crate::test_utils::create_simple_test_app_state().await;
    let config = make_composite_config();

    let request = CreateWeightedSumCompositeBeaconRequest {
        reference_beacons: vec!["not_a_valid_address".to_string()],
        weights: vec![1_000_000_000_000_000_000],
    };

    let result = create_weighted_sum_composite_beacon(&app_state, &config, &request).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("Invalid reference beacon address"),
        "Expected invalid address error, got: {err}"
    );
}

// ============================================================================
// LBCGBM BEACON CREATION TESTS (network-dependent, will fail without real factory)
// ============================================================================

#[tokio::test]
#[ignore = "requires WalletManager with Redis"]
async fn test_lbcgbm_beacon_creation_fails_without_chain() {
    let app_state = crate::test_utils::create_simple_test_app_state().await;
    let config = make_lbcgbm_config();
    let request = make_lbcgbm_request();

    // Should fail at wallet acquisition or RPC call, not panic
    let result = create_lbcgbm_beacon(&app_state, &config, &request).await;
    assert!(result.is_err());
}

#[tokio::test]
#[ignore = "requires WalletManager with Redis"]
async fn test_composite_beacon_creation_fails_without_chain() {
    let app_state = crate::test_utils::create_simple_test_app_state().await;
    let config = make_composite_config();

    let request = CreateWeightedSumCompositeBeaconRequest {
        reference_beacons: vec![
            "0x1111111111111111111111111111111111111111".to_string(),
            "0x2222222222222222222222222222222222222222".to_string(),
        ],
        weights: vec![500_000_000_000_000_000, 500_000_000_000_000_000],
    };

    // Passes validation but fails at wallet/RPC level
    let result = create_weighted_sum_composite_beacon(&app_state, &config, &request).await;
    assert!(result.is_err());
}

// ============================================================================
// CREATE AND REGISTER FACTORY BEACON TESTS
// ============================================================================

#[tokio::test]
async fn test_create_and_register_no_registry() {
    use the_beaconator::services::beacon::factory::create_and_register_factory_beacon;

    let app_state = crate::test_utils::create_simple_test_app_state().await;
    let config = BeaconTypeConfig {
        slug: "lbcgbm".to_string(),
        name: "LBCGBM".to_string(),
        description: None,
        factory_address: Address::from_str("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
        factory_type: FactoryType::LBCGBM,
        registry_address: None, // No registry
        enabled: true,
        created_at: 0,
        updated_at: 0,
    };

    let beacon_address = Address::from_str("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee").unwrap();

    let result = create_and_register_factory_beacon(&app_state, &config, beacon_address).await;
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.beacon_type, "lbcgbm");
    assert!(!response.registered); // No registry means not registered
    assert!(response.safe_proposal_hash.is_none());
    assert!(response.beacon_address.contains("eeeeeeee"));
}

#[tokio::test]
async fn test_create_and_register_with_registry_fails_gracefully() {
    use the_beaconator::services::beacon::factory::create_and_register_factory_beacon;

    let app_state = crate::test_utils::create_simple_test_app_state().await;
    let config = BeaconTypeConfig {
        slug: "weighted-sum-composite".to_string(),
        name: "WeightedSumComposite".to_string(),
        description: None,
        factory_address: Address::from_str("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap(),
        factory_type: FactoryType::WeightedSumComposite,
        registry_address: Some(
            Address::from_str("0xcccccccccccccccccccccccccccccccccccccccc").unwrap(),
        ),
        enabled: true,
        created_at: 0,
        updated_at: 0,
    };

    let beacon_address = Address::from_str("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee").unwrap();

    // Registration will fail (no real chain), but should still return the beacon data
    let result = create_and_register_factory_beacon(&app_state, &config, beacon_address).await;
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.beacon_type, "weighted-sum-composite");
    assert!(!response.registered); // Registration failed but beacon data returned
    assert!(response.beacon_address.contains("eeeeeeee"));
}
