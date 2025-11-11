use alloy::primitives::{Address, FixedBytes, U256};
use the_beaconator::services::transaction::events::{
    parse_beacon_created_event, parse_beacon_created_events_from_multicall,
    parse_data_updated_event, parse_maker_position_opened_event, parse_perp_created_event,
};

#[test]
fn test_event_parsing_function_signatures() {
    // Test that all event parsing functions exist and have correct signatures
    let receipt = create_simple_mock_receipt();
    let address = Address::from([1u8; 20]);
    let perp_id = FixedBytes::<32>::from([1u8; 32]);

    // These calls test the function signatures compile correctly
    let _: Result<Address, String> = parse_beacon_created_event(&receipt, address);
    let _: Result<U256, String> = parse_data_updated_event(&receipt, address);
    let _: Result<Vec<String>, String> =
        parse_beacon_created_events_from_multicall(&receipt, address, 1);
    let _: Result<FixedBytes<32>, String> = parse_perp_created_event(&receipt, address);
    let _: Result<U256, String> = parse_maker_position_opened_event(&receipt, address, perp_id);
}

#[test]
fn test_event_parsing_with_empty_receipt() {
    let receipt = create_simple_mock_receipt();
    let address = Address::from([1u8; 20]);

    // All event parsing functions should fail with empty receipts
    let result = parse_beacon_created_event(&receipt, address);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("BeaconCreated event not found")
    );

    let result = parse_data_updated_event(&receipt, address);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("DataUpdated event not found"));

    let result = parse_perp_created_event(&receipt, address);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("PerpCreated event not found"));

    let perp_id = FixedBytes::<32>::from([1u8; 32]);
    let result = parse_maker_position_opened_event(&receipt, address, perp_id);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("PositionOpened event (maker) not found")
    );
}

#[test]
fn test_multicall_beacon_events_parsing() {
    let receipt = create_simple_mock_receipt();
    let address = Address::from([1u8; 20]);

    // Test with zero expected count
    let result = parse_beacon_created_events_from_multicall(&receipt, address, 0);
    assert!(result.is_ok());
    let events = result.unwrap();
    assert!(events.is_empty());

    // Test with non-zero expected count - should fail because expected 5 but found 0
    let result = parse_beacon_created_events_from_multicall(&receipt, address, 5);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("Expected 5 BeaconCreated events")
    );
}

#[test]
fn test_address_patterns_in_event_parsing() {
    let receipt = create_simple_mock_receipt();

    // Test various address patterns
    let addresses = vec![
        Address::from([0u8; 20]),   // Zero address
        Address::from([255u8; 20]), // Max address
        Address::from([170u8; 20]), // Alternating bits (0xAA)
        Address::from([85u8; 20]),  // Alternating bits (0x55)
    ];

    for address in addresses {
        // Should consistently return "not found" errors for empty receipts
        assert!(parse_beacon_created_event(&receipt, address).is_err());
        assert!(parse_data_updated_event(&receipt, address).is_err());
        assert!(parse_perp_created_event(&receipt, address).is_err());
        assert!(
            parse_maker_position_opened_event(&receipt, address, FixedBytes::<32>::from([0u8; 32]))
                .is_err()
        );
    }
}

#[test]
fn test_perp_id_patterns() {
    let receipt = create_simple_mock_receipt();
    let address = Address::from([1u8; 20]);

    // Test different perp ID patterns
    let perp_ids = vec![
        FixedBytes::<32>::from([0u8; 32]),   // Zero
        FixedBytes::<32>::from([1u8; 32]),   // All ones
        FixedBytes::<32>::from([255u8; 32]), // All max
        FixedBytes::<32>::from([42u8; 32]),  // Arbitrary value
    ];

    for perp_id in perp_ids {
        let result = parse_maker_position_opened_event(&receipt, address, perp_id);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("PositionOpened event (maker) not found")
        );
    }
}

#[test]
fn test_error_message_content() {
    let receipt = create_simple_mock_receipt();
    let address = Address::from([1u8; 20]);

    // Test that error messages are descriptive
    let beacon_result = parse_beacon_created_event(&receipt, address);
    assert!(beacon_result.is_err());
    let beacon_error = beacon_result.unwrap_err();
    assert!(beacon_error.contains("BeaconCreated"));
    assert!(beacon_error.contains("not found"));

    let data_result = parse_data_updated_event(&receipt, address);
    assert!(data_result.is_err());
    let data_error = data_result.unwrap_err();
    assert!(data_error.contains("DataUpdated"));
    assert!(data_error.contains("not found"));

    let perp_result = parse_perp_created_event(&receipt, address);
    assert!(perp_result.is_err());
    let perp_error = perp_result.unwrap_err();
    assert!(perp_error.contains("PerpCreated"));
    assert!(perp_error.contains("not found"));

    let maker_result =
        parse_maker_position_opened_event(&receipt, address, FixedBytes::<32>::from([0u8; 32]));
    assert!(maker_result.is_err());
    let maker_error = maker_result.unwrap_err();
    assert!(maker_error.contains("PositionOpened"));
    assert!(maker_error.contains("not found"));
}

// Helper function to create simple mock receipts
fn create_simple_mock_receipt() -> alloy::rpc::types::TransactionReceipt {
    use alloy::consensus::{Eip658Value, Receipt, ReceiptEnvelope, ReceiptWithBloom};

    alloy::rpc::types::TransactionReceipt {
        transaction_hash: alloy::primitives::B256::ZERO,
        transaction_index: Some(0),
        block_hash: Some(alloy::primitives::B256::ZERO),
        block_number: Some(1000),
        from: Address::from([3u8; 20]),
        to: Some(Address::from([4u8; 20])),
        gas_used: 21000u64,
        effective_gas_price: 1000000000u128,
        blob_gas_used: None,
        blob_gas_price: None,
        contract_address: None,
        inner: ReceiptEnvelope::Legacy(ReceiptWithBloom {
            receipt: Receipt {
                status: Eip658Value::Eip658(true),
                cumulative_gas_used: 21000u64,
                logs: vec![], // Empty logs for testing "not found" scenarios
            },
            logs_bloom: Default::default(),
        }),
    }
}

// Build a receipt with a single log from a non-matching address (placeholder)
fn create_receipt_with_foreign_log() -> alloy::rpc::types::TransactionReceipt {
    // For now reuse empty logs to deterministically hit not-found branches
    create_simple_mock_receipt()
}

#[test]
fn test_event_parsing_address_mismatch_still_not_found() {
    let receipt = create_receipt_with_foreign_log();
    let non_emitting_factory = Address::from([9u8; 20]);
    let non_emitting_beacon = Address::from([8u8; 20]);
    let perp_id = FixedBytes::<32>::from([7u8; 32]);

    assert!(parse_beacon_created_event(&receipt, non_emitting_factory).is_err());
    assert!(parse_data_updated_event(&receipt, non_emitting_beacon).is_err());
    assert!(parse_perp_created_event(&receipt, non_emitting_factory).is_err());
    assert!(parse_maker_position_opened_event(&receipt, non_emitting_factory, perp_id).is_err());
}
