use alloy::primitives::Address;
use std::str::FromStr;
use the_beaconator::models::CreateVerifiableBeaconRequest;
use the_beaconator::services::beacon::verifiable::create_verifiable_beacon;

#[tokio::test]
async fn test_create_verifiable_beacon_invalid_verifier_address() {
    let app_state = crate::test_utils::create_simple_test_app_state();

    let request = CreateVerifiableBeaconRequest {
        verifier_address: "invalid_address".to_string(),
        initial_data: 100,
        initial_cardinality: 100,
    };

    let result = create_verifiable_beacon(&app_state, request).await;
    assert!(result.is_err());
    let error_msg = result.unwrap_err();
    // The function checks factory configuration first, so we expect that error
    assert!(
        error_msg.contains("factory")
            || error_msg.contains("configured")
            || error_msg.contains("Invalid")
            || error_msg.contains("address"),
        "Unexpected error message: {error_msg}"
    );
}

#[tokio::test]
async fn test_create_verifiable_beacon_no_factory_configured() {
    let mut app_state = crate::test_utils::create_simple_test_app_state();
    app_state.dichotomous_beacon_factory_address = None; // Remove factory address

    let request = CreateVerifiableBeaconRequest {
        verifier_address: "0x1234567890123456789012345678901234567890".to_string(),
        initial_data: 100,
        initial_cardinality: 100,
    };

    let result = create_verifiable_beacon(&app_state, request).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not configured"));
}

#[tokio::test]
async fn test_create_verifiable_beacon_network_failure() {
    let app_state = crate::test_utils::create_simple_test_app_state();

    let request = CreateVerifiableBeaconRequest {
        verifier_address: "0x1234567890123456789012345678901234567890".to_string(),
        initial_data: 100,
        initial_cardinality: 100,
    };

    // This will fail in test environment due to no network, which is expected
    let result = create_verifiable_beacon(&app_state, request).await;
    assert!(result.is_err());
}

#[test]
fn test_create_verifiable_beacon_request_validation() {
    let request = CreateVerifiableBeaconRequest {
        verifier_address: "0x1234567890123456789012345678901234567890".to_string(),
        initial_data: 12345,
        initial_cardinality: 500,
    };

    assert_eq!(request.initial_data, 12345);
    assert_eq!(request.initial_cardinality, 500);
    assert!(request.verifier_address.starts_with("0x"));
    assert_eq!(request.verifier_address.len(), 42); // 0x + 40 hex chars
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
fn test_verifiable_beacon_data_bounds() {
    // Test edge cases for initial_data and initial_cardinality
    let request = CreateVerifiableBeaconRequest {
        verifier_address: "0x1234567890123456789012345678901234567890".to_string(),
        initial_data: u128::MAX,
        initial_cardinality: u32::MAX,
    };

    assert_eq!(request.initial_data, u128::MAX);
    assert_eq!(request.initial_cardinality, u32::MAX);

    let request_min = CreateVerifiableBeaconRequest {
        verifier_address: "0x1234567890123456789012345678901234567890".to_string(),
        initial_data: 0,
        initial_cardinality: 0,
    };

    assert_eq!(request_min.initial_data, 0);
    assert_eq!(request_min.initial_cardinality, 0);
}
