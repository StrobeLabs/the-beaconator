use alloy::primitives::Address;
use std::str::FromStr;
use the_beaconator::models::CreateBeaconWithEcdsaRequest;
use the_beaconator::services::beacon::verifiable::create_verifiable_beacon_with_factory;

#[tokio::test]
#[ignore = "requires WalletManager with Redis"]
async fn test_create_verifiable_beacon_with_factory_network_failure() {
    let app_state = crate::test_utils::create_simple_test_app_state().await;
    let factory_address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let verifier_address = Address::from_str("0x9876543210987654321098765432109876543210").unwrap();

    // This will fail in test environment due to no network, which is expected
    let result = create_verifiable_beacon_with_factory(
        &app_state,
        factory_address,
        verifier_address,
        100,
        100,
    )
    .await;
    assert!(result.is_err());
}

#[test]
fn test_create_beacon_with_ecdsa_request_validation() {
    let request = CreateBeaconWithEcdsaRequest {
        beacon_type: "verifiable-twap".to_string(),
        initial_data: 12345,
        initial_cardinality: 500,
    };

    assert_eq!(request.beacon_type, "verifiable-twap");
    assert_eq!(request.initial_data, 12345);
    assert_eq!(request.initial_cardinality, 500);
}

#[test]
fn test_address_parsing_edge_cases() {
    // Test various address formats
    let valid_addresses = vec![
        "0x1234567890123456789012345678901234567890",
        "0x0000000000000000000000000000000000000000",
        "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
    ];

    for addr_str in valid_addresses {
        let result = Address::from_str(addr_str);
        assert!(result.is_ok(), "Failed to parse address: {addr_str}");
    }

    let invalid_addresses = vec![
        "not_an_address",
        "0x123",                                      // Too short
        "",                                           // Empty
        "0xZZZZ567890123456789012345678901234567890", // Invalid hex
    ];

    for addr_str in invalid_addresses {
        let result = Address::from_str(addr_str);
        assert!(result.is_err(), "Should have failed to parse: {addr_str}");
    }
}

#[test]
fn test_ecdsa_request_data_bounds() {
    // Test edge cases for initial_data and initial_cardinality
    let request = CreateBeaconWithEcdsaRequest {
        beacon_type: "verifiable-twap".to_string(),
        initial_data: u128::MAX,
        initial_cardinality: u32::MAX,
    };

    assert_eq!(request.initial_data, u128::MAX);
    assert_eq!(request.initial_cardinality, u32::MAX);

    let request_min = CreateBeaconWithEcdsaRequest {
        beacon_type: "verifiable-twap".to_string(),
        initial_data: 0,
        initial_cardinality: 0,
    };

    assert_eq!(request_min.initial_data, 0);
    assert_eq!(request_min.initial_cardinality, 0);
}

#[test]
fn test_ecdsa_request_serialization() {
    let request = CreateBeaconWithEcdsaRequest {
        beacon_type: "verifiable-twap".to_string(),
        initial_data: 1000000,
        initial_cardinality: 100,
    };

    let serialized = serde_json::to_string(&request).unwrap();
    let deserialized: CreateBeaconWithEcdsaRequest = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.beacon_type, "verifiable-twap");
    assert_eq!(deserialized.initial_data, 1000000);
    assert_eq!(deserialized.initial_cardinality, 100);
}
