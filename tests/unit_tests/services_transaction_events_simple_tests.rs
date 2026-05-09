use alloy::primitives::{Address, U256};
use the_beaconator::services::transaction::events::{
    PerpCreatedEvent, parse_index_updated_event, parse_maker_opened_event, parse_perp_created_event,
};

#[test]
fn test_event_parsing_function_signatures() {
    // Test that all event parsing functions exist with the v0.1.0 signatures.
    let receipt = create_simple_mock_receipt();
    let address = Address::from([1u8; 20]);

    let _: Result<U256, String> = parse_index_updated_event(&receipt, address);
    let _: Result<PerpCreatedEvent, String> = parse_perp_created_event(&receipt, address);
    let _: Result<U256, String> = parse_maker_opened_event(&receipt, address);
}

#[test]
fn test_event_parsing_with_empty_receipt() {
    let receipt = create_simple_mock_receipt();
    let address = Address::from([1u8; 20]);

    let result = parse_index_updated_event(&receipt, address);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("IndexUpdated event not found"));

    let result = parse_perp_created_event(&receipt, address);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("PerpCreated event not found"));

    let result = parse_maker_opened_event(&receipt, address);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("MakerOpened event not found"));
}

#[test]
fn test_address_patterns_in_event_parsing() {
    let receipt = create_simple_mock_receipt();

    let addresses = vec![
        Address::from([0u8; 20]),
        Address::from([255u8; 20]),
        Address::from([170u8; 20]),
        Address::from([85u8; 20]),
    ];

    for address in addresses {
        assert!(parse_index_updated_event(&receipt, address).is_err());
        assert!(parse_perp_created_event(&receipt, address).is_err());
        assert!(parse_maker_opened_event(&receipt, address).is_err());
    }
}

#[test]
fn test_error_message_content() {
    let receipt = create_simple_mock_receipt();
    let address = Address::from([1u8; 20]);

    let index_error = parse_index_updated_event(&receipt, address).unwrap_err();
    assert!(index_error.contains("IndexUpdated"));
    assert!(index_error.contains("not found"));

    let perp_error = parse_perp_created_event(&receipt, address).unwrap_err();
    assert!(perp_error.contains("PerpCreated"));
    assert!(perp_error.contains("not found"));

    let maker_error = parse_maker_opened_event(&receipt, address).unwrap_err();
    assert!(maker_error.contains("MakerOpened"));
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
                logs: vec![],
            },
            logs_bloom: Default::default(),
        }),
    }
}

#[test]
fn test_event_parsing_address_mismatch_still_not_found() {
    let receipt = create_simple_mock_receipt();
    let non_emitting_factory = Address::from([9u8; 20]);
    let non_emitting_beacon = Address::from([8u8; 20]);

    assert!(parse_index_updated_event(&receipt, non_emitting_beacon).is_err());
    assert!(parse_perp_created_event(&receipt, non_emitting_factory).is_err());
    assert!(parse_maker_opened_event(&receipt, non_emitting_factory).is_err());
}
