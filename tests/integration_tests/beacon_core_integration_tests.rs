use alloy::primitives::{Address, B256};
use serial_test::serial;
use std::str::FromStr;
use std::time::Duration;
use tokio::time::timeout;

use the_beaconator::models::UpdateBeaconRequest;
use the_beaconator::services::beacon::core::{
    create_beacon_via_factory, is_beacon_registered, is_transaction_confirmed,
    register_beacon_with_registry, update_beacon,
};

/// Test beacon creation via factory with Anvil
#[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_create_beacon_via_factory_with_anvil() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let owner_address = app_state.wallet_address;
    let factory_address = app_state.beacon_factory_address;

    // Create beacon via factory - this should execute actual contract calls
    let result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;

    // In integration test, this should succeed with real contract deployment
    assert!(
        result.is_ok(),
        "Factory beacon creation should succeed: {:?}",
        result
    );

    if let Ok(beacon_address) = result {
        // Verify the beacon address is valid
        assert_ne!(beacon_address, Address::ZERO);
        println!("Created beacon at address: {}", beacon_address);

        // Test that we can verify the beacon was created
        let is_registered = is_beacon_registered(
            &app_state,
            beacon_address,
            app_state.perpcity_registry_address,
        )
        .await;
        assert!(is_registered.is_ok());
    }
}

/// Test beacon registration with registry
#[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_register_beacon_with_registry_integration() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // First create a beacon
    let owner_address = app_state.wallet_address;
    let factory_address = app_state.beacon_factory_address;

    let beacon_result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;
    assert!(beacon_result.is_ok(), "Beacon creation should succeed");

    let beacon_address = beacon_result.unwrap();
    let registry_address = app_state.perpcity_registry_address;

    // Register the beacon with registry
    let register_result =
        register_beacon_with_registry(&app_state, beacon_address, registry_address).await;

    assert!(
        register_result.is_ok(),
        "Beacon registration should succeed: {:?}",
        register_result
    );

    // Verify registration
    let is_registered = is_beacon_registered(&app_state, beacon_address, registry_address).await;
    assert!(is_registered.is_ok());
    assert!(
        is_registered.unwrap(),
        "Beacon should be registered after registration"
    );
}

/// Test update beacon with proof
#[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_update_beacon_integration() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Create a beacon first
    let owner_address = app_state.wallet_address;
    let factory_address = app_state.beacon_factory_address;

    let beacon_result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;
    assert!(beacon_result.is_ok(), "Beacon creation should succeed");

    let beacon_address = beacon_result.unwrap();

    // Create update request
    let update_request = UpdateBeaconRequest {
        beacon_address: format!("{:?}", beacon_address),
        value: 12345,
        proof: vec![1, 2, 3, 4, 5, 6, 7, 8],
    };

    // Update beacon with proof
    let update_result = update_beacon(&app_state, update_request).await;

    // This might fail if the beacon doesn't accept arbitrary proofs, but should at least
    // get to the contract call stage
    match update_result {
        Ok(_) => println!("Beacon update succeeded"),
        Err(e) => {
            println!("Beacon update failed (expected): {}", e);
            // Should fail with contract-level error, not network error
            assert!(
                !e.contains("network"),
                "Should not be a network error: {}",
                e
            );
        }
    }
}

/// Test transaction confirmation checking
#[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_transaction_confirmation_integration() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Create a beacon to get a real transaction hash
    let owner_address = app_state.wallet_address;
    let factory_address = app_state.beacon_factory_address;

    let beacon_result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;
    assert!(beacon_result.is_ok(), "Beacon creation should succeed");

    // For this test, we'll use a known invalid transaction hash
    let invalid_tx_hash =
        B256::from_str("0x0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();

    let confirmation_result = is_transaction_confirmed(&app_state, invalid_tx_hash).await;

    // Should get a proper response from Anvil (not a network error)
    assert!(
        confirmation_result.is_ok(),
        "Should get response from Anvil: {:?}",
        confirmation_result
    );
    // Expect None for invalid tx
    assert!(
        confirmation_result.unwrap().is_none(),
        "Invalid transaction should not be confirmed"
    );
}

/// Test beacon registration check with various addresses
#[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_beacon_registration_check_integration() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Test with unregistered beacon
    let unregistered_beacon =
        Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let registry_address = app_state.perpcity_registry_address;

    let is_registered =
        is_beacon_registered(&app_state, unregistered_beacon, registry_address).await;
    assert!(
        is_registered.is_ok(),
        "Should get response from Anvil: {:?}",
        is_registered
    );
    assert!(
        !is_registered.unwrap(),
        "Random address should not be registered"
    );

    // Test with zero address
    let zero_beacon = Address::ZERO;
    let is_zero_registered = is_beacon_registered(&app_state, zero_beacon, registry_address).await;
    assert!(
        is_zero_registered.is_ok(),
        "Should get response for zero address"
    );
    assert!(
        !is_zero_registered.unwrap(),
        "Zero address should not be registered"
    );
}

/// Test multiple beacon creation in sequence
#[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_multiple_beacon_creation_sequence() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let owner_address = app_state.wallet_address;
    let factory_address = app_state.beacon_factory_address;

    let mut beacon_addresses = Vec::new();

    // Create multiple beacons
    for i in 0..3 {
        println!("Creating beacon {}", i);

        let beacon_result =
            create_beacon_via_factory(&app_state, owner_address, factory_address).await;
        assert!(
            beacon_result.is_ok(),
            "Beacon {} creation should succeed: {:?}",
            i,
            beacon_result
        );

        let beacon_address = beacon_result.unwrap();
        assert_ne!(
            beacon_address,
            Address::ZERO,
            "Beacon {} address should not be zero",
            i
        );

        // Each beacon should have a unique address
        assert!(
            !beacon_addresses.contains(&beacon_address),
            "Beacon {} should have unique address",
            i
        );
        beacon_addresses.push(beacon_address);

        println!("Created beacon {} at address: {}", i, beacon_address);
    }

    assert_eq!(
        beacon_addresses.len(),
        3,
        "Should have created 3 unique beacons"
    );
}

/// Test error handling with invalid parameters
#[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_beacon_operations_error_handling() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Test with zero addresses
    let zero_address = Address::ZERO;
    let factory_address = app_state.beacon_factory_address;

    // Creating beacon with zero owner should handle gracefully
    let result = create_beacon_via_factory(&app_state, zero_address, factory_address).await;
    match result {
        Ok(_) => println!("Zero owner beacon creation unexpectedly succeeded"),
        Err(e) => println!("Zero owner beacon creation failed as expected: {}", e),
    }

    // Test with zero factory address
    let owner_address = app_state.wallet_address;
    let result = create_beacon_via_factory(&app_state, owner_address, zero_address).await;
    assert!(result.is_err(), "Zero factory address should fail");

    // Test invalid update request
    let invalid_update = UpdateBeaconRequest {
        beacon_address: "invalid_address".to_string(),
        value: 100,
        proof: vec![1, 2, 3, 4],
    };

    let update_result = update_beacon(&app_state, invalid_update).await;
    assert!(
        update_result.is_err(),
        "Invalid address should fail parsing"
    );
    assert!(
        update_result
            .unwrap_err()
            .contains("Invalid beacon address")
    );
}

/// Test timeout handling for long operations
#[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_beacon_operation_timeouts() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let owner_address = app_state.wallet_address;
    let factory_address = app_state.beacon_factory_address;

    // Test beacon creation with timeout
    let result = timeout(
        Duration::from_secs(30),
        create_beacon_via_factory(&app_state, owner_address, factory_address),
    )
    .await;

    assert!(
        result.is_ok(),
        "Beacon creation should complete within timeout"
    );

    let beacon_result = result.unwrap();
    assert!(
        beacon_result.is_ok(),
        "Beacon creation should succeed: {:?}",
        beacon_result
    );
}

/// Test concurrent beacon operations
#[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_concurrent_beacon_operations() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let owner_address = app_state.wallet_address;
    let factory_address = app_state.beacon_factory_address;

    // Create multiple beacons concurrently
    let mut handles = Vec::new();

    for i in 0..3 {
        let app_state_clone = app_state.clone();
        let handle = tokio::spawn(async move {
            println!("Starting concurrent beacon creation {}", i);
            let result =
                create_beacon_via_factory(&app_state_clone, owner_address, factory_address).await;
            (i, result)
        });
        handles.push(handle);
    }

    // Wait for all to complete
    let mut beacon_addresses = Vec::new();
    for handle in handles {
        let (i, result) = handle.await.unwrap();
        println!("Concurrent beacon {} result: {:?}", i, result);

        if let Ok(beacon_address) = result {
            assert_ne!(beacon_address, Address::ZERO);
            beacon_addresses.push(beacon_address);
        }
    }

    // Should have created at least some beacons (serialization might limit concurrency)
    assert!(
        !beacon_addresses.is_empty(),
        "Should have created at least one beacon"
    );

    // All addresses should be unique
    beacon_addresses.sort();
    beacon_addresses.dedup();
    println!(
        "Created {} unique beacons concurrently",
        beacon_addresses.len()
    );
}
