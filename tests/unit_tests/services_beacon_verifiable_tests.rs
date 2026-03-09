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

/// Verify that with_deploy_code sets tx kind to Create,
/// which was the root cause of the "missing properties: [('Wallet', ['to'])]" bug.
#[test]
fn test_deploy_tx_uses_create_kind() {
    use alloy::network::TransactionBuilder;
    use alloy::primitives::{Address, Bytes, TxKind, U256};
    use alloy::rpc::types::TransactionRequest;
    use alloy::sol_types::SolValue;

    let bytecode = vec![0x60, 0x80, 0x60, 0x40]; // minimal placeholder bytecode
    let verifier = Address::ZERO;
    let initial_index = 100u128;

    let constructor_args = (verifier, U256::from(initial_index)).abi_encode();
    let mut deploy_data = bytecode;
    deploy_data.extend_from_slice(&constructor_args);

    let tx = TransactionRequest::default().with_deploy_code(Bytes::from(deploy_data));

    // with_deploy_code explicitly sets TxKind::Create
    assert_eq!(
        tx.to,
        Some(TxKind::Create),
        "Deploy tx must have TxKind::Create"
    );
    // input should be populated
    assert!(tx.input.input().is_some(), "Deploy tx must have input data");
}

/// Verify that the old .input() approach leaves tx kind unset (the bug).
#[test]
fn test_old_input_approach_lacks_create_kind() {
    use alloy::primitives::{Bytes, TxKind};
    use alloy::rpc::types::TransactionRequest;

    let deploy_data = vec![0x60, 0x80, 0x60, 0x40];
    let tx = TransactionRequest::default().input(Bytes::from(deploy_data).into());

    // The old approach sets input but does NOT set tx kind to Create.
    // This was the bug: wallet layer expected explicit Create kind.
    assert_ne!(
        tx.to,
        Some(TxKind::Create),
        "Old .input() should NOT set Create kind"
    );
}

#[tokio::test]
async fn test_deploy_identity_beacon_empty_bytecode_in_test_state() {
    let app_state = crate::test_utils::create_simple_test_app_state().await;
    // Test app state should have empty bytecode (deploy_identity_beacon would reject this)
    assert!(
        app_state.identity_beacon_bytecode.is_empty(),
        "Test app state should have empty bytecode"
    );
}
