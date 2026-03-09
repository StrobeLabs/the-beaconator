// Factory beacon tests - LBCGBM and WeightedSumComposite beacon types

use alloy::primitives::Address;
use std::str::FromStr;
use the_beaconator::models::beacon_type::{BeaconTypeConfig, FactoryType};
use the_beaconator::models::requests::{
    CreateLBCGBMBeaconRequest, CreateWeightedSumCompositeBeaconRequest,
};
use the_beaconator::models::responses::CreateBeaconResponse;

// ============================================================================
// LBCGBM REQUEST SERIALIZATION TESTS
// ============================================================================

#[test]
fn test_lbcgbm_request_serialization_roundtrip() {
    let request = CreateLBCGBMBeaconRequest {
        measurement_scale: 1_000_000_000_000_000_000, // 1e18
        sigma_base: 50_000_000_000_000_000,           // 0.05e18
        scaling_factor: 1_000_000_000_000_000_000,
        alpha: 500_000_000_000_000_000, // 0.5e18
        decay: 950_000_000_000_000_000, // 0.95e18
        initial_sigma_ratio: 1_000_000_000_000_000_000,
        variance_scaling: true,
        min_index: 100_000_000_000_000_000_000,    // 100e18
        max_index: 10_000_000_000_000_000_000_000, // 10000e18
        steepness: 1_000_000_000_000_000_000,
        initial_index: 1_000_000_000_000_000_000_000, // 1000e18
    };

    let json = serde_json::to_string(&request).unwrap();
    let deserialized: CreateLBCGBMBeaconRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.measurement_scale, request.measurement_scale);
    assert_eq!(deserialized.sigma_base, request.sigma_base);
    assert_eq!(deserialized.scaling_factor, request.scaling_factor);
    assert_eq!(deserialized.alpha, request.alpha);
    assert_eq!(deserialized.decay, request.decay);
    assert_eq!(
        deserialized.initial_sigma_ratio,
        request.initial_sigma_ratio
    );
    assert_eq!(deserialized.variance_scaling, request.variance_scaling);
    assert_eq!(deserialized.min_index, request.min_index);
    assert_eq!(deserialized.max_index, request.max_index);
    assert_eq!(deserialized.steepness, request.steepness);
    assert_eq!(deserialized.initial_index, request.initial_index);
}

#[test]
fn test_lbcgbm_request_from_json() {
    let json = r#"{
        "measurement_scale": 1000000000000000000,
        "sigma_base": 50000000000000000,
        "scaling_factor": 1000000000000000000,
        "alpha": 500000000000000000,
        "decay": 950000000000000000,
        "initial_sigma_ratio": 1000000000000000000,
        "variance_scaling": false,
        "min_index": 100000000000000000000,
        "max_index": 10000000000000000000000,
        "steepness": 1000000000000000000,
        "initial_index": 1000000000000000000000
    }"#;

    let request: CreateLBCGBMBeaconRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.measurement_scale, 1_000_000_000_000_000_000);
    assert!(!request.variance_scaling);
    assert_eq!(request.initial_index, 1_000_000_000_000_000_000_000);
}

#[test]
fn test_lbcgbm_request_zero_values() {
    let request = CreateLBCGBMBeaconRequest {
        measurement_scale: 0,
        sigma_base: 0,
        scaling_factor: 0,
        alpha: 0,
        decay: 0,
        initial_sigma_ratio: 0,
        variance_scaling: false,
        min_index: 0,
        max_index: 0,
        steepness: 0,
        initial_index: 0,
    };

    let json = serde_json::to_string(&request).unwrap();
    let deserialized: CreateLBCGBMBeaconRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.initial_index, 0);
}

#[test]
fn test_lbcgbm_request_max_u128_values() {
    let request = CreateLBCGBMBeaconRequest {
        measurement_scale: u128::MAX,
        sigma_base: u128::MAX,
        scaling_factor: u128::MAX,
        alpha: u128::MAX,
        decay: u128::MAX,
        initial_sigma_ratio: u128::MAX,
        variance_scaling: true,
        min_index: u128::MAX,
        max_index: u128::MAX,
        steepness: u128::MAX,
        initial_index: u128::MAX,
    };

    let json = serde_json::to_string(&request).unwrap();
    let deserialized: CreateLBCGBMBeaconRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.measurement_scale, u128::MAX);
    assert_eq!(deserialized.initial_index, u128::MAX);
}

// ============================================================================
// WEIGHTED SUM COMPOSITE REQUEST SERIALIZATION TESTS
// ============================================================================

#[test]
fn test_composite_request_serialization_roundtrip() {
    let request = CreateWeightedSumCompositeBeaconRequest {
        reference_beacons: vec![
            "0x1111111111111111111111111111111111111111".to_string(),
            "0x2222222222222222222222222222222222222222".to_string(),
        ],
        weights: vec![
            500_000_000_000_000_000, // 0.5 WAD
            500_000_000_000_000_000, // 0.5 WAD
        ],
    };

    let json = serde_json::to_string(&request).unwrap();
    let deserialized: CreateWeightedSumCompositeBeaconRequest =
        serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.reference_beacons.len(), 2);
    assert_eq!(deserialized.weights.len(), 2);
    assert_eq!(
        deserialized.reference_beacons[0],
        "0x1111111111111111111111111111111111111111"
    );
    assert_eq!(deserialized.weights[0], 500_000_000_000_000_000);
}

#[test]
fn test_composite_request_from_json() {
    let json = r#"{
        "reference_beacons": [
            "0x1111111111111111111111111111111111111111",
            "0x2222222222222222222222222222222222222222",
            "0x3333333333333333333333333333333333333333"
        ],
        "weights": [
            333333333333333333,
            333333333333333333,
            333333333333333334
        ]
    }"#;

    let request: CreateWeightedSumCompositeBeaconRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.reference_beacons.len(), 3);
    assert_eq!(request.weights.len(), 3);
}

#[test]
fn test_composite_request_single_beacon() {
    let request = CreateWeightedSumCompositeBeaconRequest {
        reference_beacons: vec!["0x1111111111111111111111111111111111111111".to_string()],
        weights: vec![1_000_000_000_000_000_000], // 1.0 WAD
    };

    let json = serde_json::to_string(&request).unwrap();
    let deserialized: CreateWeightedSumCompositeBeaconRequest =
        serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.reference_beacons.len(), 1);
    assert_eq!(deserialized.weights.len(), 1);
}

#[test]
fn test_composite_request_empty_arrays() {
    let request = CreateWeightedSumCompositeBeaconRequest {
        reference_beacons: vec![],
        weights: vec![],
    };

    // Serialization should work (validation is at the service layer)
    let json = serde_json::to_string(&request).unwrap();
    let deserialized: CreateWeightedSumCompositeBeaconRequest =
        serde_json::from_str(&json).unwrap();
    assert!(deserialized.reference_beacons.is_empty());
    assert!(deserialized.weights.is_empty());
}

// ============================================================================
// FACTORY TYPE SERDE TESTS
// ============================================================================

#[test]
fn test_factory_type_all_variants_serde() {
    let variants = vec![
        (FactoryType::Identity, "\"Identity\""),
        (FactoryType::LBCGBM, "\"LBCGBM\""),
        (
            FactoryType::WeightedSumComposite,
            "\"WeightedSumComposite\"",
        ),
    ];

    for (variant, expected_json) in variants {
        let json = serde_json::to_string(&variant).unwrap();
        assert_eq!(json, expected_json);

        let deserialized: FactoryType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, variant);
    }
}

#[test]
fn test_factory_type_invalid_variant() {
    let result = serde_json::from_str::<FactoryType>("\"NonExistent\"");
    assert!(result.is_err());
}

// ============================================================================
// BEACON TYPE CONFIG WITH NEW FACTORY TYPES
// ============================================================================

#[test]
fn test_beacon_type_config_lbcgbm() {
    let config = BeaconTypeConfig {
        slug: "lbcgbm".to_string(),
        name: "LBCGBM".to_string(),
        description: Some(
            "Standalone beacon with Identity preprocessor + CGBM basefn + Bounded transform"
                .to_string(),
        ),
        factory_address: Address::from_str("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
        factory_type: FactoryType::LBCGBM,
        registry_address: Some(
            Address::from_str("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap(),
        ),
        enabled: true,
        created_at: 1700000000,
        updated_at: 1700000000,
    };

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: BeaconTypeConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.slug, "lbcgbm");
    assert_eq!(deserialized.factory_type, FactoryType::LBCGBM);
    assert!(deserialized.enabled);
}

#[test]
fn test_beacon_type_config_weighted_sum_composite() {
    let config = BeaconTypeConfig {
        slug: "weighted-sum-composite".to_string(),
        name: "WeightedSumComposite".to_string(),
        description: Some("Composite beacon with WeightedSum composer".to_string()),
        factory_address: Address::from_str("0xcccccccccccccccccccccccccccccccccccccccc").unwrap(),
        factory_type: FactoryType::WeightedSumComposite,
        registry_address: None,
        enabled: true,
        created_at: 1700000000,
        updated_at: 1700000000,
    };

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: BeaconTypeConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.slug, "weighted-sum-composite");
    assert_eq!(deserialized.factory_type, FactoryType::WeightedSumComposite);
    assert!(deserialized.registry_address.is_none());
}

// ============================================================================
// CREATE BEACON RESPONSE WITH FACTORY TYPES
// ============================================================================

#[test]
fn test_create_beacon_response_lbcgbm_type() {
    let response = CreateBeaconResponse {
        beacon_address: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        beacon_type: "lbcgbm".to_string(),
        factory_address: "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
        registered: true,
        safe_proposal_hash: None,
    };

    let json = serde_json::to_string(&response).unwrap();
    let deserialized: CreateBeaconResponse = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.beacon_type, "lbcgbm");
    assert!(deserialized.registered);
    assert!(deserialized.safe_proposal_hash.is_none());
    // safe_proposal_hash should be omitted from JSON when None
    assert!(!json.contains("safe_proposal_hash"));
}

#[test]
fn test_create_beacon_response_composite_unregistered() {
    let response = CreateBeaconResponse {
        beacon_address: "0xcccccccccccccccccccccccccccccccccccccccc".to_string(),
        beacon_type: "weighted-sum-composite".to_string(),
        factory_address: "0xdddddddddddddddddddddddddddddddddddddd".to_string(),
        registered: false,
        safe_proposal_hash: None,
    };

    let json = serde_json::to_string(&response).unwrap();
    let deserialized: CreateBeaconResponse = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.beacon_type, "weighted-sum-composite");
    assert!(!deserialized.registered);
}

#[test]
fn test_create_beacon_response_with_safe_proposal() {
    let response = CreateBeaconResponse {
        beacon_address: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        beacon_type: "lbcgbm".to_string(),
        factory_address: "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
        registered: false,
        safe_proposal_hash: Some(
            "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string(),
        ),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("safe_proposal_hash"));
    let deserialized: CreateBeaconResponse = serde_json::from_str(&json).unwrap();
    assert!(!deserialized.registered);
    assert!(deserialized.safe_proposal_hash.is_some());
}
