use alloy::primitives::Address;
use serial_test::serial;
use std::str::FromStr;

use the_beaconator::services::beacon::verifiable::create_verifiable_beacon_with_factory;

/// Test verifiable beacon creation with Anvil
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_create_verifiable_beacon_with_factory_integration() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let factory_address = app_state.beacon_factory_address;
    let verifier_address = app_state.funding_wallet_address;

    let result = create_verifiable_beacon_with_factory(
        &app_state,
        factory_address,
        verifier_address,
        12345,
        10,
    )
    .await;

    // This might fail if dichotomous factory contract doesn't exist, but should
    // get past the validation stage
    match result {
        Ok(beacon_address) => {
            println!("Verifiable beacon creation succeeded: {beacon_address}");
        }
        Err(e) => {
            println!("Verifiable beacon creation failed (may be expected): {e}");
        }
    }
}

/// Test verifiable beacon creation with edge case values
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_create_verifiable_beacon_with_factory_edge_cases() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let factory_address = app_state.beacon_factory_address;

    // Test with zero address verifier
    let zero_verifier = Address::from_str("0x0000000000000000000000000000000000000000").unwrap();
    let result =
        create_verifiable_beacon_with_factory(&app_state, factory_address, zero_verifier, 0, 1)
            .await;
    match result {
        Ok(_) => println!("Zero address verifier succeeded"),
        Err(e) => println!("Zero address verifier failed: {e}"),
    }

    // Test with max values
    let max_verifier = Address::from_str("0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF").unwrap();
    let result = create_verifiable_beacon_with_factory(
        &app_state,
        factory_address,
        max_verifier,
        u128::MAX,
        u32::MAX,
    )
    .await;
    match result {
        Ok(_) => println!("Max values succeeded"),
        Err(e) => println!("Max values failed: {e}"),
    }
}

/// Test multiple verifiable beacon operations concurrently
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_concurrent_verifiable_beacon_operations() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let factory_address = app_state.beacon_factory_address;
    let verifier_address = app_state.funding_wallet_address;

    let mut handles = Vec::new();

    // Start multiple create operations
    for i in 0..3u128 {
        let app_state_clone = app_state.clone();
        let handle = tokio::spawn(async move {
            let result = create_verifiable_beacon_with_factory(
                &app_state_clone,
                factory_address,
                verifier_address,
                1000 + i,
                10 + i as u32,
            )
            .await;
            (i, result)
        });
        handles.push(handle);
    }

    // Wait for all operations
    let mut success_count = 0;
    for handle in handles {
        let (i, result) = handle.await.unwrap();
        match result {
            Ok(beacon_address) => {
                println!("Concurrent verifiable beacon {i} succeeded: {beacon_address}");
                success_count += 1;
            }
            Err(e) => println!("Concurrent verifiable beacon {i} failed: {e}"),
        }
    }

    println!("Concurrent verifiable beacon operations: {success_count} successes");
}

/// Test verifiable beacon operations with extreme values
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_verifiable_beacon_extreme_values() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let factory_address = app_state.beacon_factory_address;
    let verifier_address = app_state.funding_wallet_address;

    // Test with minimum values
    let min_result =
        create_verifiable_beacon_with_factory(&app_state, factory_address, verifier_address, 0, 0)
            .await;
    match min_result {
        Ok(_) => println!("Minimum values succeeded"),
        Err(e) => println!("Minimum values failed: {e}"),
    }

    // Test with maximum values
    let max_result = create_verifiable_beacon_with_factory(
        &app_state,
        factory_address,
        verifier_address,
        u128::MAX,
        u32::MAX,
    )
    .await;
    match max_result {
        Ok(_) => println!("Maximum values succeeded"),
        Err(e) => println!("Maximum values failed: {e}"),
    }
}
