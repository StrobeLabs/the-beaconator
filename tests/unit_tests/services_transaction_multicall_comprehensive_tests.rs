use alloy::primitives::{Address, B256, Bytes};
use alloy::rpc::types::TransactionReceipt;
use std::str::FromStr;
use the_beaconator::routes::IMulticall3;
use the_beaconator::services::transaction::multicall::{
    build_multicall_call, execute_batch_beacon_creation_multicall,
    execute_batch_liquidity_deposit_multicall, execute_multicall, parse_multicall_results,
    validate_multicall_success,
};

#[tokio::test]
async fn test_execute_multicall_empty_calls() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider);
    let multicall_address =
        Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap();
    let calls = vec![];

    let result = execute_multicall(&app_state, multicall_address, calls).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("No calls provided"));
}

#[tokio::test]
async fn test_execute_multicall_single_call() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider);
    let multicall_address =
        Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap();

    let call = build_multicall_call(
        Address::from_str("0x1234567890123456789012345678901234567890").unwrap(),
        Bytes::from(vec![0x01, 0x02, 0x03]),
        false,
    );
    let calls = vec![call];

    // Should fail deterministically due to mock provider
    let result = execute_multicall(&app_state, multicall_address, calls).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_multicall_multiple_calls() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider);
    let multicall_address =
        Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap();

    let calls = vec![
        build_multicall_call(
            Address::from_str("0x1234567890123456789012345678901234567890").unwrap(),
            Bytes::from(vec![0x01, 0x02, 0x03]),
            false,
        ),
        build_multicall_call(
            Address::from_str("0x0987654321098765432109876543210987654321").unwrap(),
            Bytes::from(vec![0x04, 0x05, 0x06]),
            true,
        ),
        build_multicall_call(
            Address::from_str("0xABCDEF1234567890123456789012345678901234").unwrap(),
            Bytes::from(vec![0x07, 0x08, 0x09, 0x0A]),
            false,
        ),
    ];

    // Should fail deterministically due to mock provider
    let result = execute_multicall(&app_state, multicall_address, calls).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_batch_beacon_creation_multicall_empty() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider);
    let multicall_address =
        Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap();
    let calls = vec![];

    let result =
        execute_batch_beacon_creation_multicall(&app_state, multicall_address, calls).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("No beacon creation calls provided")
    );
}

#[tokio::test]
async fn test_execute_batch_beacon_creation_multicall_network_failure() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider);
    let multicall_address =
        Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap();

    let calls = vec![build_multicall_call(
        Address::from_str("0x1234567890123456789012345678901234567890").unwrap(),
        Bytes::from(vec![0x01, 0x02, 0x03]),
        false,
    )];

    // Should fail deterministically due to mock provider
    let result =
        execute_batch_beacon_creation_multicall(&app_state, multicall_address, calls).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_batch_liquidity_deposit_multicall_empty() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider);
    let multicall_address =
        Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap();
    let calls = vec![];

    let result =
        execute_batch_liquidity_deposit_multicall(&app_state, multicall_address, calls).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("No liquidity deposit calls provided")
    );
}

#[tokio::test]
async fn test_execute_batch_liquidity_deposit_multicall_network_failure() {
    let mock_provider = crate::test_utils::create_mock_provider_with_network_error();
    let app_state = crate::test_utils::create_test_app_state_with_provider(mock_provider);
    let multicall_address =
        Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap();

    let calls = vec![
        build_multicall_call(
            Address::from_str("0x1234567890123456789012345678901234567890").unwrap(),
            Bytes::from(vec![0x01, 0x02, 0x03]),
            true,
        ),
        build_multicall_call(
            Address::from_str("0x0987654321098765432109876543210987654321").unwrap(),
            Bytes::from(vec![0x04, 0x05, 0x06]),
            false,
        ),
    ];

    // Should fail deterministically due to mock provider
    let result =
        execute_batch_liquidity_deposit_multicall(&app_state, multicall_address, calls).await;
    assert!(result.is_err());
}

#[test]
fn test_build_multicall_call_allow_failure_true() {
    let target = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let calldata = Bytes::from(vec![0x01, 0x02, 0x03, 0x04]);
    let allow_failure = true;

    let call = build_multicall_call(target, calldata.clone(), allow_failure);

    assert_eq!(call.target, target);
    assert_eq!(call.callData, calldata);
    assert!(call.allowFailure);
}

#[test]
fn test_build_multicall_call_allow_failure_false() {
    let target = Address::from_str("0x0987654321098765432109876543210987654321").unwrap();
    let calldata = Bytes::from(vec![0x05, 0x06, 0x07]);
    let allow_failure = false;

    let call = build_multicall_call(target, calldata.clone(), allow_failure);

    assert_eq!(call.target, target);
    assert_eq!(call.callData, calldata);
    assert!(!call.allowFailure);
}

#[test]
fn test_build_multicall_call_empty_calldata() {
    let target = Address::from_str("0xABCDEF1234567890123456789012345678901234").unwrap();
    let calldata = Bytes::from(vec![]);
    let allow_failure = false;

    let call = build_multicall_call(target, calldata.clone(), allow_failure);

    assert_eq!(call.target, target);
    assert_eq!(call.callData, calldata);
    assert!(!call.allowFailure);
}

#[test]
fn test_build_multicall_call_large_calldata() {
    let target = Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
    let calldata = Bytes::from(vec![0xFF; 1000]); // Large calldata
    let allow_failure = true;

    let call = build_multicall_call(target, calldata.clone(), allow_failure);

    assert_eq!(call.target, target);
    assert_eq!(call.callData, calldata);
    assert!(call.allowFailure);
}

#[test]
fn test_parse_multicall_results_zero_expected() {
    let receipt = create_mock_receipt();
    let expected_count = 0;

    let result = parse_multicall_results(&receipt, expected_count);
    assert!(result.is_ok());
    let results = result.unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_parse_multicall_results_multiple_expected() {
    let receipt = create_mock_receipt();
    let expected_count = 5;

    let result = parse_multicall_results(&receipt, expected_count);
    assert!(result.is_ok());
    let results = result.unwrap();
    // Currently returns empty vector as it's a placeholder implementation
    assert_eq!(results.len(), 0);
}

#[test]
fn test_parse_multicall_results_large_expected_count() {
    let receipt = create_mock_receipt();
    let expected_count = 1000;

    let result = parse_multicall_results(&receipt, expected_count);
    assert!(result.is_ok());
    let results = result.unwrap();
    // Currently returns empty vector as it's a placeholder implementation
    assert_eq!(results.len(), 0);
}

#[test]
fn test_validate_multicall_success_empty_results() {
    let results = vec![];

    let result = validate_multicall_success(&results);
    assert!(result.is_ok());
}

#[test]
fn test_validate_multicall_success_all_successful() {
    let results = vec![
        (true, Bytes::from(vec![0x01, 0x02])),
        (true, Bytes::from(vec![0x03, 0x04])),
        (true, Bytes::from(vec![0x05, 0x06])),
    ];

    let result = validate_multicall_success(&results);
    assert!(result.is_ok());
}

#[test]
fn test_validate_multicall_success_with_failures() {
    let results = vec![
        (true, Bytes::from(vec![0x01, 0x02])),
        (false, Bytes::from(vec![0x03, 0x04])),
        (true, Bytes::from(vec![0x05, 0x06])),
        (false, Bytes::from(vec![0x07, 0x08])),
    ];

    let result = validate_multicall_success(&results);
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("2 failures"));
    assert!(error.contains("[1, 3]"));
}

#[test]
fn test_validate_multicall_success_all_failures() {
    let results = vec![
        (false, Bytes::from(vec![0x01, 0x02])),
        (false, Bytes::from(vec![0x03, 0x04])),
        (false, Bytes::from(vec![0x05, 0x06])),
    ];

    let result = validate_multicall_success(&results);
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("3 failures"));
    assert!(error.contains("[0, 1, 2]"));
}

#[test]
fn test_validate_multicall_success_single_failure() {
    let results = vec![
        (true, Bytes::from(vec![0x01, 0x02])),
        (true, Bytes::from(vec![0x03, 0x04])),
        (false, Bytes::from(vec![0x05, 0x06])),
        (true, Bytes::from(vec![0x07, 0x08])),
    ];

    let result = validate_multicall_success(&results);
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("1 failures"));
    assert!(error.contains("[2]"));
}

#[test]
fn test_build_multicall_call_different_addresses() {
    let addresses = vec![
        "0x0000000000000000000000000000000000000000", // Zero address
        "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF", // Max address
        "0x1234567890123456789012345678901234567890", // Normal address
        "0xDeaDbeefdEAdbeefdEadbEEFdeadbeEFdEaDbeeF", // Mixed case
    ];

    for addr_str in addresses {
        let target = Address::from_str(addr_str).unwrap();
        let calldata = Bytes::from(vec![0x01, 0x02, 0x03]);
        let allow_failure = false;

        let call = build_multicall_call(target, calldata.clone(), allow_failure);
        assert_eq!(call.target, target);
        assert_eq!(call.callData, calldata);
        assert!(!call.allowFailure);
    }
}

#[test]
fn test_multicall_call_struct_fields() {
    let target = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let calldata = Bytes::from(vec![0x01, 0x02, 0x03, 0x04]);
    let allow_failure = true;

    let call = IMulticall3::Call3 {
        target,
        callData: calldata.clone(),
        allowFailure: allow_failure,
    };

    // Test that struct fields are accessible
    assert_eq!(call.target, target);
    assert_eq!(call.callData, calldata);
    assert_eq!(call.allowFailure, allow_failure);
}

#[test]
fn test_multicall_results_with_various_byte_lengths() {
    let results = vec![
        (true, Bytes::from(vec![])),                        // Empty bytes
        (true, Bytes::from(vec![0x01])),                    // Single byte
        (true, Bytes::from(vec![0x01, 0x02, 0x03, 0x04])),  // Normal length
        (true, Bytes::from(vec![0xFF; 100])),               // Large bytes
        (false, Bytes::from(vec![0x00, 0x00, 0x00, 0x00])), // Zero bytes
    ];

    // Test validation with mixed results (one false result should cause failure)
    let validation_result = validate_multicall_success(&results);
    assert!(validation_result.is_err()); // Should fail due to one false result
}

#[test]
fn test_multicall_error_messages() {
    // Test empty calls error message
    let results_with_failures = vec![
        (false, Bytes::from(vec![0x01])),
        (false, Bytes::from(vec![0x02])),
    ];

    let error = validate_multicall_success(&results_with_failures).unwrap_err();
    assert!(error.contains("Multicall had"));
    assert!(error.contains("failures"));
    assert!(error.contains("indices"));
}

// Helper function to create mock receipts
fn create_mock_receipt() -> TransactionReceipt {
    use alloy::consensus::{Eip658Value, Receipt, ReceiptEnvelope, ReceiptWithBloom};

    TransactionReceipt {
        transaction_hash: B256::from([1u8; 32]),
        transaction_index: Some(0),
        block_hash: Some(B256::from([2u8; 32])),
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
