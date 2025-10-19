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
        proof: hex::decode("01020304").unwrap(),
        public_signals: hex::decode(
            "0000000000000000000000000000000000000000000000000000000000000064",
        )
        .unwrap(), // 100 in hex, padded to 32 bytes
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
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider);
    let tx_hash =
        B256::from_str("0x1234567890123456789012345678901234567890123456789012345678901234")
            .unwrap();

    // Should fail deterministically due to mock provider
    let result = is_transaction_confirmed(&app_state, tx_hash).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_create_beacon_via_factory_network_failure() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider);
    let owner_address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let factory_address = Address::from_str("0x0987654321098765432109876543210987654321").unwrap();

    // Should fail deterministically due to mock provider
    let result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_register_beacon_with_registry_network_failure() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider);
    let beacon_address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let registry_address = Address::from_str("0x0987654321098765432109876543210987654321").unwrap();

    // Should fail deterministically due to mock provider
    let result = register_beacon_with_registry(&app_state, beacon_address, registry_address).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_beacon_empty_address() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider);

    let request = UpdateBeaconRequest {
        beacon_address: "".to_string(),
        proof: hex::decode("01020304").unwrap(),
        public_signals: hex::decode(
            "0000000000000000000000000000000000000000000000000000000000000064",
        )
        .unwrap(),
    };

    let result = update_beacon(&app_state, request).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid beacon address"));
}

#[tokio::test]
async fn test_update_beacon_zero_address() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider);

    let request = UpdateBeaconRequest {
        beacon_address: "0x0000000000000000000000000000000000000000".to_string(),
        proof: hex::decode("01020304").unwrap(),
        public_signals: hex::decode(
            "0000000000000000000000000000000000000000000000000000000000000064",
        )
        .unwrap(),
    };

    // Valid address format, but should fail deterministically at network level
    let result = update_beacon(&app_state, request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_beacon_max_address() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider);

    let request = UpdateBeaconRequest {
        beacon_address: "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF".to_string(),
        proof: hex::decode("01020304").unwrap(),
        public_signals: hex::decode(
            "0000000000000000000000000000000000000000000000000000000000000064",
        )
        .unwrap(),
    };

    // Valid address format, but should fail deterministically at network level
    let result = update_beacon(&app_state, request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_update_beacon_various_proof_sizes() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider);

    let large_proof_bytes = vec![0xff; 100];
    let very_large_proof_bytes = vec![0x00; 1000];

    let test_proofs = vec![
        vec![],                         // Empty proof
        vec![0x00],                     // Single byte
        vec![0x01, 0x02, 0x03],         // Small proof
        large_proof_bytes.clone(),      // Large proof
        very_large_proof_bytes.clone(), // Very large proof
    ];

    for proof in test_proofs {
        let request = UpdateBeaconRequest {
            beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
            proof: proof.clone(),
            public_signals: hex::decode(
                "0000000000000000000000000000000000000000000000000000000000000064",
            )
            .unwrap(),
        };

        let result = update_beacon(&app_state, request).await;
        // Should fail deterministically at network level, not due to proof size
        assert!(result.is_err());
    }
}

#[tokio::test]
async fn test_update_beacon_various_public_signals() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider);

    let test_public_signals = vec![
        hex::decode("0000000000000000000000000000000000000000000000000000000000000000").unwrap(), // 0
        hex::decode("0000000000000000000000000000000000000000000000000000000000000001").unwrap(), // 1
        hex::decode("0000000000000000000000000000000000000000000000000000000000000064").unwrap(), // 100
        hex::decode("00000000000000000000000000000000000000000000000000000000000003e8").unwrap(), // 1000
        hex::decode("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap(), // max u256
    ];

    for public_signals in test_public_signals {
        let request = UpdateBeaconRequest {
            beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
            proof: hex::decode("01020304").unwrap(),
            public_signals: public_signals.clone(),
        };

        let result = update_beacon(&app_state, request).await;
        // Should fail deterministically at network level, not due to public signals value
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
        proof: hex::decode("0102030405").unwrap(),
        public_signals: hex::decode(
            "000000000000000000000000000000000000000000000000000000000000002a",
        )
        .unwrap(), // 42 in hex
    };

    assert_eq!(request.proof, vec![0x01, 0x02, 0x03, 0x04, 0x05]);
    assert_eq!(
        request.public_signals,
        hex::decode("000000000000000000000000000000000000000000000000000000000000002a").unwrap()
    );
    assert!(request.beacon_address.starts_with("0x"));
}

#[test]
fn test_update_beacon_request_serialization() {
    let request = UpdateBeaconRequest {
        beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
        proof: vec![0x0a, 0x14, 0x1e, 0x28, 0x32], // [10, 20, 30, 40, 50]
        public_signals: hex::decode(
            "0000000000000000000000000000000000000000000000000000000000003039",
        )
        .unwrap(), // 12345 in hex
    };

    let json = serde_json::to_string(&request).unwrap();
    let deserialized: UpdateBeaconRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.beacon_address, request.beacon_address);
    assert_eq!(deserialized.proof, request.proof);
    assert_eq!(deserialized.public_signals, request.public_signals);
}

#[test]
fn test_update_beacon_request_edge_cases() {
    // Test max u256 value in public signals
    let request_max = UpdateBeaconRequest {
        beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
        proof: vec![0xff; 1000], // Large proof
        public_signals: hex::decode(
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        )
        .unwrap(), // max u256
    };
    assert_eq!(request_max.proof.len(), 1000); // 1000 bytes
    assert_eq!(request_max.public_signals.len(), 32); // 32 bytes (256 bits)

    // Test zero value
    let request_zero = UpdateBeaconRequest {
        beacon_address: "0x0000000000000000000000000000000000000000".to_string(),
        proof: vec![], // Empty proof
        public_signals: hex::decode(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap(), // 0
    };
    assert_eq!(request_zero.proof, Vec::<u8>::new());
    assert_eq!(request_zero.public_signals.len(), 32); // 32 bytes
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
        assert!(result.is_ok(), "Failed to parse valid address: {addr_str}");
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
