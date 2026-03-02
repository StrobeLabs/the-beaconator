use alloy::primitives::Address;
use std::str::FromStr;
use the_beaconator::models::CreateBeaconWithEcdsaRequest;

#[test]
fn test_create_beacon_with_ecdsa_request_validation() {
    let request = CreateBeaconWithEcdsaRequest {
        initial_index: 12345,
    };

    assert_eq!(request.initial_index, 12345);
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
    // Test edge cases for initial_index
    let request = CreateBeaconWithEcdsaRequest {
        initial_index: u128::MAX,
    };

    assert_eq!(request.initial_index, u128::MAX);

    let request_min = CreateBeaconWithEcdsaRequest { initial_index: 0 };

    assert_eq!(request_min.initial_index, 0);
}

#[test]
fn test_ecdsa_request_serialization() {
    let request = CreateBeaconWithEcdsaRequest {
        initial_index: 1000000,
    };

    let serialized = serde_json::to_string(&request).unwrap();
    let deserialized: CreateBeaconWithEcdsaRequest = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.initial_index, 1000000);
}
