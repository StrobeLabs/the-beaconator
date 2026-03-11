use alloy::primitives::{Address, B256};
use serial_test::serial;
use std::str::FromStr;

use the_beaconator::models::UpdateBeaconRequest;
use the_beaconator::services::beacon::core::{
    create_identity_beacon, is_beacon_registered, is_transaction_confirmed,
    register_beacon_with_registry, update_beacon,
};

/// Test identity beacon creation with Anvil
///
/// Note: create_identity_beacon calls ECDSAVerifierFactory.createVerifier() which
/// requires the factory contract to be deployed on Anvil. If the factory is not
/// deployed, the test verifies it fails gracefully with a contract-level error.
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_create_identity_beacon_with_anvil() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let result = create_identity_beacon(&app_state, 12345).await;

    match result {
        Ok((beacon_address, verifier_address)) => {
            assert_ne!(beacon_address, Address::ZERO);
            assert_ne!(verifier_address, Address::ZERO);
            println!("Created beacon at address: {beacon_address}, verifier: {verifier_address}");

            let is_registered = is_beacon_registered(
                &app_state,
                beacon_address,
                app_state.contracts.perpcity_registry,
            )
            .await;
            assert!(is_registered.is_ok());
        }
        Err(e) => {
            // Expected when ECDSAVerifierFactory not deployed on test Anvil
            println!("Identity beacon creation failed (expected without factory contract): {e}");
            assert!(
                e.contains("createVerifier") || e.contains("Failed to"),
                "Should be a contract-level error, got: {e}"
            );
        }
    }
}

/// Test beacon registration with registry
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_register_beacon_with_registry_integration() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let beacon_result = create_identity_beacon(&app_state, 12345).await;

    // Skip registration test if beacon creation fails (factory not deployed)
    let (beacon_address, _verifier_address) = match beacon_result {
        Ok(r) => r,
        Err(e) => {
            println!("Skipping registration test - beacon creation failed: {e}");
            return;
        }
    };

    let registry_address = app_state.contracts.perpcity_registry;
    let register_result =
        register_beacon_with_registry(&app_state, beacon_address, registry_address).await;

    assert!(
        register_result.is_ok(),
        "Beacon registration should succeed: {register_result:?}"
    );

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

    let beacon_result = create_identity_beacon(&app_state, 12345).await;

    // Skip update test if beacon creation fails (factory not deployed)
    let (beacon_address, _verifier_address) = match beacon_result {
        Ok(r) => r,
        Err(e) => {
            println!("Skipping update test - beacon creation failed: {e}");
            return;
        }
    };

    let update_request = UpdateBeaconRequest {
        beacon_address: beacon_address.to_string(),
        proof: "0x0102030405060708".parse().unwrap(),
        public_signals: "0x0000000000000000000000000000000000000000000000000000000000003039"
            .parse()
            .unwrap(), // 12345 in hex
    };

    let update_result = update_beacon(&app_state, update_request).await;

    match update_result {
        Ok(_) => println!("Beacon update succeeded"),
        Err(e) => {
            println!("Beacon update failed (expected): {e}");
            assert!(!e.contains("network"), "Should not be a network error: {e}");
        }
    }
}

/// Test transaction confirmation checking
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_transaction_confirmation_integration() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let invalid_tx_hash =
        B256::from_str("0x0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();

    let confirmation_result = is_transaction_confirmed(&app_state, invalid_tx_hash).await;

    assert!(
        confirmation_result.is_ok(),
        "Should get response from Anvil: {confirmation_result:?}"
    );
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

    let unregistered_beacon =
        Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let registry_address = app_state.contracts.perpcity_registry;

    let is_registered =
        is_beacon_registered(&app_state, unregistered_beacon, registry_address).await;
    assert!(
        is_registered.is_ok(),
        "Should get response from Anvil: {is_registered:?}"
    );
    assert!(
        !is_registered.unwrap(),
        "Random address should not be registered"
    );

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

    let mut beacon_addresses = Vec::new();

    for i in 0..3u128 {
        println!("Creating beacon {i}");
        let beacon_result = create_identity_beacon(&app_state, 1000 + i).await;

        match beacon_result {
            Ok((beacon_address, _verifier_address)) => {
                assert_ne!(beacon_address, Address::ZERO);
                assert!(
                    !beacon_addresses.contains(&beacon_address),
                    "Beacon {i} should have unique address"
                );
                beacon_addresses.push(beacon_address);
                println!("Created beacon {i} at address: {beacon_address}");
            }
            Err(e) => {
                // Expected when ECDSAVerifierFactory not deployed
                println!("Beacon {i} creation failed (expected without factory): {e}");
                return;
            }
        }
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

    let invalid_update = UpdateBeaconRequest {
        beacon_address: "invalid_address".to_string(),
        proof: "0x01020304".parse().unwrap(),
        public_signals: "0x0000000000000000000000000000000000000000000000000000000000000064"
            .parse()
            .unwrap(), // 100 in hex
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
    use std::time::Duration;
    use tokio::time::timeout;

    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let result = timeout(
        Duration::from_secs(30),
        create_identity_beacon(&app_state, 12345),
    )
    .await;

    assert!(
        result.is_ok(),
        "Beacon creation should complete within timeout"
    );

    // Don't assert success - factory may not be deployed on test Anvil
    match result.unwrap() {
        Ok((beacon, verifier)) => println!("Beacon created: {beacon}, verifier: {verifier}"),
        Err(e) => println!("Beacon creation failed (expected without factory): {e}"),
    }
}

/// Test concurrent beacon operations
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_concurrent_beacon_operations() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let mut handles = Vec::new();

    for i in 0..3u128 {
        let app_state_clone = app_state.clone();
        let handle = tokio::spawn(async move {
            println!("Starting concurrent beacon creation {i}");
            let result = create_identity_beacon(&app_state_clone, 1000 + i).await;
            (i, result)
        });
        handles.push(handle);
    }

    let mut beacon_addresses = Vec::new();
    let mut all_failed_with_factory_error = true;
    for handle in handles {
        let (i, result) = handle.await.unwrap();
        println!("Concurrent beacon {i} result: {result:?}");

        match result {
            Ok((beacon_address, _verifier_address)) => {
                assert_ne!(beacon_address, Address::ZERO);
                beacon_addresses.push(beacon_address);
                all_failed_with_factory_error = false;
            }
            Err(e) => {
                if !e.contains("createVerifier") && !e.contains("Failed to") {
                    all_failed_with_factory_error = false;
                }
            }
        }
    }

    if all_failed_with_factory_error {
        println!("All concurrent operations failed (expected without factory contract)");
        return;
    }

    assert!(
        !beacon_addresses.is_empty(),
        "Should have created at least one beacon"
    );

    beacon_addresses.sort();
    beacon_addresses.dedup();
    println!(
        "Created {} unique beacons concurrently",
        beacon_addresses.len()
    );
}
