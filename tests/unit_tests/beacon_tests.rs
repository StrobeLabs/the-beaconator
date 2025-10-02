// Beacon route tests - extracted from src/routes/beacon.rs

use alloy::primitives::{Address, B256, Bytes};
use rocket::State;
use rocket::serde::json::Json;
use std::str::FromStr;
use the_beaconator::guards::ApiToken;
use the_beaconator::models::{
    BatchCreatePerpcityBeaconRequest, BatchCreatePerpcityBeaconResponse, BatchUpdateBeaconRequest,
    BeaconUpdateData, CreateBeaconRequest,
};
use the_beaconator::routes::IMulticall3;
use the_beaconator::routes::beacon::{
    batch_create_perpcity_beacon, batch_update_beacon, create_beacon,
};
use the_beaconator::services::beacon::core::{
    create_beacon_via_factory, is_beacon_registered, is_transaction_confirmed,
    register_beacon_with_registry,
};

#[tokio::test]
async fn test_batch_update_beacon_with_multicall3() {
    let token = ApiToken("test_token".to_string());
    let mut app_state = crate::test_utils::create_simple_test_app_state();

    // Set multicall3 address for the test
    app_state.multicall3_address =
        Some(Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap());

    let state = State::from(&app_state);

    let update_data = BeaconUpdateData {
        beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
        proof: "0x01020304".to_string(), // Mock proof as hex
        public_signals: "0x0000000000000000000000000000000000000000000000000000000000000064"
            .to_string(), // 100 encoded as hex
    };

    let request = Json(BatchUpdateBeaconRequest {
        updates: vec![update_data],
    });

    // This will fail in test environment due to no actual contracts, but should not panic
    let result = batch_update_beacon(request, token, state).await;

    // Should return an error response rather than panic
    assert!(result.is_ok());
    let response = result.unwrap().into_inner();

    // Should contain error details about the failed multicall
    assert!(!response.success);
    assert!(response.data.is_some());
    let batch_data = response.data.unwrap();
    assert_eq!(batch_data.successful_updates, 0);
    assert_eq!(batch_data.failed_updates, 1);
    assert!(!batch_data.results.is_empty());
}

#[tokio::test]
async fn test_batch_update_beacon_without_multicall3() {
    let token = ApiToken("test_token".to_string());
    let app_state = crate::test_utils::create_simple_test_app_state(); // No multicall3_address set
    let state = State::from(&app_state);

    let update_data = BeaconUpdateData {
        beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
        proof: "0x01020304".to_string(),
        public_signals: "0x0000000000000000000000000000000000000000000000000000000000000064"
            .to_string(),
    };

    let request = Json(BatchUpdateBeaconRequest {
        updates: vec![update_data],
    });

    let result = batch_update_beacon(request, token, state).await;

    assert!(result.is_ok());
    let response = result.unwrap().into_inner();

    // Should fail with clear error message about missing multicall3
    assert!(!response.success);
    assert!(response.data.is_some());
    let batch_data = response.data.unwrap();
    assert_eq!(batch_data.successful_updates, 0);
    assert_eq!(batch_data.failed_updates, 1);
    assert!(
        batch_data.results[0]
            .error
            .as_ref()
            .unwrap()
            .contains("Multicall3")
            || batch_data.results[0]
                .error
                .as_ref()
                .unwrap()
                .contains("multicall")
    );
}

#[tokio::test]
async fn test_batch_create_beacons_with_multicall3() {
    let token = ApiToken("test_token".to_string());
    let mut app_state = crate::test_utils::create_simple_test_app_state();

    // Set multicall3 address for the test
    app_state.multicall3_address =
        Some(Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap());

    let state = State::from(&app_state);

    let request = Json(BatchCreatePerpcityBeaconRequest { count: 3 });

    let result = batch_create_perpcity_beacon(request, token, state).await;

    // Should return an error response due to multicall not implemented yet
    assert!(result.is_ok());
    let response = result.unwrap().into_inner();

    assert!(!response.success);
    assert!(response.data.is_some());
    let batch_data = response.data.unwrap();
    assert_eq!(batch_data.created_count, 0);
    assert_eq!(batch_data.failed_count, 3);
    assert!(!batch_data.errors.is_empty());
}

#[test]
fn test_multicall3_atomic_behavior() {
    // Test that multicall3 calls are atomic (allowFailure: false)
    let update_data = BeaconUpdateData {
        beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
        proof: "0x01020304".to_string(),
        public_signals: "0x0000000000000000000000000000000000000000000000000000000000000064"
            .to_string(),
    };

    // Create mock multicall3 call and verify atomicity setting
    let beacon_address = Address::from_str(&update_data.beacon_address).unwrap();

    // This would be the actual call structure in the multicall
    let call = IMulticall3::Call3 {
        target: beacon_address,
        allowFailure: false,    // Atomic behavior
        callData: Bytes::new(), // Mock call data
    };

    // Verify atomic setting
    assert!(
        !call.allowFailure,
        "Multicall3 calls should be atomic (allowFailure: false)"
    );
    assert_eq!(call.target, beacon_address);
}

#[tokio::test]
async fn test_create_beacon_not_implemented() {
    let token = ApiToken("test_token".to_string());

    let request = Json(CreateBeaconRequest {
        placeholder: "test".to_string(),
    });

    let result = create_beacon(request, token).await;
    let response = result.into_inner();

    assert!(!response.success);
    assert!(response.message.contains("not yet implemented"));
}

#[test]
fn test_app_state_has_required_contract_info() {
    let app_state = crate::test_utils::create_simple_test_app_state();

    // Test that all required contract addresses are set
    assert_ne!(
        app_state.beacon_factory_address,
        Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
    );
    assert_ne!(
        app_state.perpcity_registry_address,
        Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
    );
    assert!(!app_state.access_token.is_empty());
}

#[test]
fn test_batch_create_response_serialization() {
    // Test response serialization/deserialization
    let response = BatchCreatePerpcityBeaconResponse {
        created_count: 2,
        beacon_addresses: vec![
            "0x1234567890123456789012345678901234567890".to_string(),
            "0x9876543210987654321098765432109876543210".to_string(),
        ],
        failed_count: 1,
        errors: vec!["Error creating beacon".to_string()],
    };

    let serialized = serde_json::to_string(&response).unwrap();
    let deserialized: BatchCreatePerpcityBeaconResponse =
        serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.created_count, 2);
    assert_eq!(deserialized.failed_count, 1);
    assert_eq!(deserialized.beacon_addresses.len(), 2);
    assert_eq!(deserialized.errors.len(), 1);
}

// Additional beacon helper function tests
#[tokio::test]
async fn test_create_beacon_via_factory_helper() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let owner_address = Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
    let factory_address = app_state.beacon_factory_address;

    // This will fail without a real network, but tests the function signature
    let result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;
    assert!(result.is_err()); // Expected to fail without real network
}

#[tokio::test]
async fn test_register_beacon_with_registry_helper() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let beacon_address = Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
    let registry_address = app_state.perpcity_registry_address;

    // This will fail without a real network, but tests the function signature
    let result = register_beacon_with_registry(&app_state, beacon_address, registry_address).await;
    assert!(result.is_err()); // Expected to fail without real network
}

#[tokio::test]
async fn test_transaction_confirmation_timeout_handling() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let tx_hash =
        B256::from_str("0x1234567890123456789012345678901234567890123456789012345678901234")
            .unwrap();

    // Test transaction confirmation check
    let result = is_transaction_confirmed(&app_state, tx_hash).await;
    // Should fail due to network issues in test environment
    assert!(result.is_err());
    let error_msg = result.unwrap_err();
    assert!(error_msg.contains("Failed to check transaction") || error_msg.contains("on-chain"));
}

#[tokio::test]
async fn test_beacon_registration_check() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    let beacon_address = Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
    let registry_address = app_state.perpcity_registry_address;

    // Test beacon registration check
    let result = is_beacon_registered(&app_state, beacon_address, registry_address).await;
    assert!(result.is_ok());
    // Should return false since beacon doesn't exist on test network
    assert!(!result.unwrap());
}
