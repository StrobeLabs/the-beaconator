use alloy::primitives::{Address, B256};
use std::str::FromStr;
use the_beaconator::models::UpdateBeaconRequest;
use the_beaconator::services::beacon::core::{
    create_beacon_via_factory, is_beacon_registered, is_transaction_confirmed,
    register_beacon_with_registry, update_beacon,
};

#[tokio::test]
async fn test_update_beacon_invalid_address() {
    let app_state = crate::test_utils::create_simple_test_app_state();

    let request = UpdateBeaconRequest {
        beacon_address: "invalid_address".to_string(),
        value: 100,
        proof: vec![1, 2, 3, 4],
    };

    let result = update_beacon(&app_state, request).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid beacon address"));
}

#[tokio::test]
async fn test_is_beacon_registered_with_mock_state() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let beacon_address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let registry_address = Address::from_str("0x0987654321098765432109876543210987654321").unwrap();

    // This will fail in test environment due to no network, which is expected
    let result = is_beacon_registered(&app_state, beacon_address, registry_address).await;
    // The result might be Ok(false) instead of Err in test environment
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_is_transaction_confirmed_with_mock_state() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let tx_hash =
        B256::from_str("0x1234567890123456789012345678901234567890123456789012345678901234")
            .unwrap();

    // This will fail in test environment due to no network, which is expected
    let result = is_transaction_confirmed(&app_state, tx_hash).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_beacon_via_factory_network_failure() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let owner_address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let factory_address = Address::from_str("0x0987654321098765432109876543210987654321").unwrap();

    // This will fail in test environment due to no network, which is expected
    let result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_register_beacon_with_registry_network_failure() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let beacon_address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let registry_address = Address::from_str("0x0987654321098765432109876543210987654321").unwrap();

    // This will fail in test environment due to no network, which is expected
    let result = register_beacon_with_registry(&app_state, beacon_address, registry_address).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_beacon_empty_address() {
    let app_state = crate::test_utils::create_simple_test_app_state();

    let request = UpdateBeaconRequest {
        beacon_address: "".to_string(),
        value: 100,
        proof: vec![1, 2, 3, 4],
    };

    let result = update_beacon(&app_state, request).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid beacon address"));
}

#[tokio::test]
async fn test_update_beacon_zero_address() {
    let app_state = crate::test_utils::create_simple_test_app_state();

    let request = UpdateBeaconRequest {
        beacon_address: "0x0000000000000000000000000000000000000000".to_string(),
        value: 100,
        proof: vec![1, 2, 3, 4],
    };

    // Valid address format, but will fail at network level
    let result = update_beacon(&app_state, request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_beacon_max_address() {
    let app_state = crate::test_utils::create_simple_test_app_state();

    let request = UpdateBeaconRequest {
        beacon_address: "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF".to_string(),
        value: 100,
        proof: vec![1, 2, 3, 4],
    };

    // Valid address format, but will fail at network level
    let result = update_beacon(&app_state, request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_beacon_various_proof_sizes() {
    let app_state = crate::test_utils::create_simple_test_app_state();

    let test_proofs = vec![
        vec![],           // Empty proof
        vec![0],          // Single byte
        vec![1, 2, 3],    // Small proof
        vec![0xFF; 100],  // Large proof
        vec![0x00; 1000], // Very large proof
    ];

    for proof in test_proofs {
        let request = UpdateBeaconRequest {
            beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
            value: 100,
            proof: proof.clone(),
        };

        let result = update_beacon(&app_state, request).await;
        // Should fail at network level, not due to proof size
        assert!(result.is_err());
    }
}

#[tokio::test]
async fn test_update_beacon_various_values() {
    let app_state = crate::test_utils::create_simple_test_app_state();

    let test_values = vec![0, 1, 100, 1000, u64::MAX];

    for value in test_values {
        let request = UpdateBeaconRequest {
            beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
            value,
            proof: vec![1, 2, 3, 4],
        };

        let result = update_beacon(&app_state, request).await;
        // Should fail at network level, not due to value
        assert!(result.is_err());
    }
}

#[tokio::test]
async fn test_create_beacon_via_factory_zero_owner() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let owner_address = Address::from_str("0x0000000000000000000000000000000000000000").unwrap();
    let factory_address = Address::from_str("0x0987654321098765432109876543210987654321").unwrap();

    let result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_beacon_via_factory_zero_factory() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let owner_address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let factory_address = Address::from_str("0x0000000000000000000000000000000000000000").unwrap();

    let result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_beacon_via_factory_same_addresses() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();

    let result = create_beacon_via_factory(&app_state, address, address).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_is_beacon_registered_zero_addresses() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let zero_address = Address::from_str("0x0000000000000000000000000000000000000000").unwrap();

    // Test zero beacon address
    let result = is_beacon_registered(&app_state, zero_address, zero_address).await;
    assert!(result.is_ok());

    // Test zero registry address
    let beacon_address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let result = is_beacon_registered(&app_state, beacon_address, zero_address).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_is_beacon_registered_same_addresses() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();

    let result = is_beacon_registered(&app_state, address, address).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_is_transaction_confirmed_zero_hash() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let zero_hash = B256::from([0u8; 32]);

    let result = is_transaction_confirmed(&app_state, zero_hash).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_is_transaction_confirmed_max_hash() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let max_hash = B256::from([0xFFu8; 32]);

    let result = is_transaction_confirmed(&app_state, max_hash).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_is_transaction_confirmed_various_hashes() {
    let app_state = crate::test_utils::create_simple_test_app_state();

    let test_hashes = vec![
        B256::from([0u8; 32]),   // All zeros
        B256::from([255u8; 32]), // All ones
        B256::from([170u8; 32]), // Alternating bits (0xAA)
        B256::from([85u8; 32]),  // Alternating bits (0x55)
        B256::from([1u8; 32]),   // Mostly zeros with one bit
    ];

    for hash in test_hashes {
        let result = is_transaction_confirmed(&app_state, hash).await;
        assert!(result.is_err()); // Should fail in test environment
    }
}

#[tokio::test]
async fn test_register_beacon_with_registry_zero_addresses() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let zero_address = Address::from_str("0x0000000000000000000000000000000000000000").unwrap();

    // Test zero beacon address
    let registry_address = Address::from_str("0x0987654321098765432109876543210987654321").unwrap();
    let result = register_beacon_with_registry(&app_state, zero_address, registry_address).await;
    assert!(result.is_err());

    // Test zero registry address
    let beacon_address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let result = register_beacon_with_registry(&app_state, beacon_address, zero_address).await;
    assert!(result.is_err());

    // Test both zero
    let result = register_beacon_with_registry(&app_state, zero_address, zero_address).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_register_beacon_with_registry_same_addresses() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();

    let result = register_beacon_with_registry(&app_state, address, address).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_register_beacon_with_registry_max_addresses() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let max_address = Address::from_str("0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF").unwrap();

    let result = register_beacon_with_registry(&app_state, max_address, max_address).await;
    assert!(result.is_err());
}

#[test]
fn test_update_beacon_request_validation() {
    let request = UpdateBeaconRequest {
        beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
        value: 42,
        proof: vec![1, 2, 3, 4, 5],
    };

    assert_eq!(request.value, 42);
    assert_eq!(request.proof, vec![1, 2, 3, 4, 5]);
    assert!(request.beacon_address.starts_with("0x"));
}

#[test]
fn test_update_beacon_request_serialization() {
    let request = UpdateBeaconRequest {
        beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
        value: 12345,
        proof: vec![10, 20, 30, 40, 50],
    };

    let json = serde_json::to_string(&request).unwrap();
    let deserialized: UpdateBeaconRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.beacon_address, request.beacon_address);
    assert_eq!(deserialized.value, request.value);
    assert_eq!(deserialized.proof, request.proof);
}

#[test]
fn test_update_beacon_request_edge_cases() {
    // Test max value
    let request_max = UpdateBeaconRequest {
        beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
        value: u64::MAX,
        proof: vec![255u8; 1000],
    };
    assert_eq!(request_max.value, u64::MAX);
    assert_eq!(request_max.proof.len(), 1000);

    // Test zero value
    let request_zero = UpdateBeaconRequest {
        beacon_address: "0x0000000000000000000000000000000000000000".to_string(),
        value: 0,
        proof: vec![],
    };
    assert_eq!(request_zero.value, 0);
    assert_eq!(request_zero.proof.len(), 0);
}

#[test]
fn test_address_parsing_edge_cases() {
    use alloy::primitives::Address;
    use std::str::FromStr;

    // Test valid addresses
    let valid_addresses = vec![
        "0x0000000000000000000000000000000000000000",
        "0x1234567890123456789012345678901234567890",
        "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
        "0xDeaDbeefdEAdbeefdEadbEEFdeadbeEFdEaDbeeF", // Mixed case
    ];

    for addr_str in valid_addresses {
        let result = Address::from_str(addr_str);
        assert!(
            result.is_ok(),
            "Failed to parse valid address: {addr_str}"
        );
    }

    // Test invalid addresses
    let invalid_addresses = vec![
        "invalid_address",
        "0x123",                                      // Too short
        "",                                           // Empty
        "0xZZZZ567890123456789012345678901234567890", // Invalid hex
        "12345678901234567890123456789012345678901",  // Too long (41 chars)
    ];

    for addr_str in invalid_addresses {
        let result = Address::from_str(addr_str);
        assert!(result.is_err(), "Should have failed to parse: {addr_str}");
    }
}

#[test]
fn test_transaction_hash_edge_cases() {
    use alloy::primitives::B256;
    use std::str::FromStr;

    // Test valid hashes
    let valid_hashes = vec![
        "0x0000000000000000000000000000000000000000000000000000000000000000",
        "0x1234567890123456789012345678901234567890123456789012345678901234",
        "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
        "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
    ];

    for hash_str in valid_hashes {
        let result = B256::from_str(hash_str);
        assert!(result.is_ok(), "Failed to parse valid hash: {hash_str}");
    }

    // Test invalid hashes
    let invalid_hashes = vec![
        "invalid_hash",
        "0x123",                                                              // Too short
        "",                                                                   // Empty
        "0xZZZZ567890123456789012345678901234567890123456789012345678901234", // Invalid hex
        "12345678901234567890123456789012345678901234567890123456789012345",  // Too long (65 chars)
    ];

    for hash_str in invalid_hashes {
        let result = B256::from_str(hash_str);
        assert!(result.is_err(), "Should have failed to parse: {hash_str}");
    }
}
