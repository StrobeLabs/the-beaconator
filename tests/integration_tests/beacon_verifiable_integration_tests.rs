use serial_test::serial;

use the_beaconator::models::CreateVerifiableBeaconRequest;
use the_beaconator::services::beacon::verifiable::create_verifiable_beacon;

/// Test verifiable beacon creation with Anvil
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_create_verifiable_beacon_integration() {
    let (mut app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Set up dichotomous factory address (use beacon factory for test)
    app_state.dichotomous_beacon_factory_address = Some(app_state.beacon_factory_address);

    let request = CreateVerifiableBeaconRequest {
        verifier_address: format!("{:?}", app_state.wallet_address),
        initial_data: 12345,
        initial_cardinality: 10,
    };

    let result = create_verifiable_beacon(&app_state, request).await;

    // This might fail if dichotomous factory contract doesn't exist, but should
    // get past the validation stage
    match result {
        Ok(tx_hash) => {
            println!("Verifiable beacon creation succeeded: {}", tx_hash);
            assert!(tx_hash.starts_with("0x"), "Should return transaction hash");
        }
        Err(e) => {
            println!("Verifiable beacon creation failed (may be expected): {}", e);
            // Should not be a validation error
            assert!(
                !e.contains("Invalid verifier address"),
                "Should not be validation error: {}",
                e
            );
            assert!(
                !e.contains("not configured"),
                "Factory should be configured: {}",
                e
            );
        }
    }
}

/// Test verifiable beacon creation without factory configured
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_create_verifiable_beacon_no_factory() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Don't set dichotomous factory address (should be None by default)
    assert!(app_state.dichotomous_beacon_factory_address.is_none());

    let request = CreateVerifiableBeaconRequest {
        verifier_address: format!("{:?}", app_state.wallet_address),
        initial_data: 12345,
        initial_cardinality: 10,
    };

    let result = create_verifiable_beacon(&app_state, request).await;

    assert!(result.is_err(), "Should fail without factory configured");
    assert!(
        result.unwrap_err().contains("not configured"),
        "Should mention factory not configured"
    );
}

/// Test verifiable beacon creation with invalid verifier address
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_create_verifiable_beacon_invalid_verifier() {
    let (mut app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    app_state.dichotomous_beacon_factory_address = Some(app_state.beacon_factory_address);

    let request = CreateVerifiableBeaconRequest {
        verifier_address: "invalid_address".to_string(),
        initial_data: 12345,
        initial_cardinality: 10,
    };

    let result = create_verifiable_beacon(&app_state, request).await;

    assert!(result.is_err(), "Should fail with invalid verifier address");
    assert!(result.unwrap_err().contains("Invalid verifier address"));
}

/// Test verifiable beacon creation with edge case values
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_create_verifiable_beacon_edge_cases() {
    let (mut app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    app_state.dichotomous_beacon_factory_address = Some(app_state.beacon_factory_address);

    // Test with zero address verifier
    let zero_request = CreateVerifiableBeaconRequest {
        verifier_address: "0x0000000000000000000000000000000000000000".to_string(),
        initial_data: 0,
        initial_cardinality: 1,
    };

    let result = create_verifiable_beacon(&app_state, zero_request).await;
    match result {
        Ok(_) => println!("Zero address verifier succeeded"),
        Err(e) => println!("Zero address verifier failed: {}", e),
    }

    // Test with max values
    let max_request = CreateVerifiableBeaconRequest {
        verifier_address: "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF".to_string(),
        initial_data: u128::MAX,
        initial_cardinality: u32::MAX,
    };

    let result = create_verifiable_beacon(&app_state, max_request).await;
    match result {
        Ok(_) => println!("Max values succeeded"),
        Err(e) => println!("Max values failed: {}", e),
    }
}

/// Test verifiable beacon operations with zero address
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_verifiable_beacon_zero_address_handling() {
    let (mut app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    app_state.dichotomous_beacon_factory_address = Some(app_state.beacon_factory_address);

    // Test create with zero verifier
    let create_request = CreateVerifiableBeaconRequest {
        verifier_address: "0x0000000000000000000000000000000000000000".to_string(),
        initial_data: 1,
        initial_cardinality: 1,
    };

    let create_result = create_verifiable_beacon(&app_state, create_request).await;
    match create_result {
        Ok(_) => println!("Create with zero verifier succeeded"),
        Err(e) => println!("Create with zero verifier failed: {}", e),
    }
}

/// Test multiple verifiable beacon operations concurrently
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_concurrent_verifiable_beacon_operations() {
    let (mut app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    app_state.dichotomous_beacon_factory_address = Some(app_state.beacon_factory_address);

    let mut handles = Vec::new();

    // Start multiple create operations
    for i in 0..3 {
        let app_state_clone = app_state.clone();
        let handle = tokio::spawn(async move {
            let request = CreateVerifiableBeaconRequest {
                verifier_address: format!("{:?}", app_state_clone.wallet_address),
                initial_data: 1000 + i,
                initial_cardinality: 10 + i as u32,
            };

            let result = create_verifiable_beacon(&app_state_clone, request).await;
            (i, result)
        });
        handles.push(handle);
    }

    // Wait for all operations
    let mut success_count = 0;
    for handle in handles {
        let (i, result) = handle.await.unwrap();
        match result {
            Ok(tx_hash) => {
                println!("Concurrent verifiable beacon {} succeeded: {}", i, tx_hash);
                success_count += 1;
            }
            Err(e) => println!("Concurrent verifiable beacon {} failed: {}", i, e),
        }
    }

    println!(
        "Concurrent verifiable beacon operations: {} successes",
        success_count
    );
}

/// Test verifiable beacon operations with extreme values
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_verifiable_beacon_extreme_values() {
    let (mut app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    app_state.dichotomous_beacon_factory_address = Some(app_state.beacon_factory_address);

    // Test with minimum values
    let min_request = CreateVerifiableBeaconRequest {
        verifier_address: format!("{:?}", app_state.wallet_address),
        initial_data: 0,
        initial_cardinality: 0,
    };

    let min_result = create_verifiable_beacon(&app_state, min_request).await;
    match min_result {
        Ok(_) => println!("Minimum values succeeded"),
        Err(e) => println!("Minimum values failed: {}", e),
    }

    // Test with maximum values
    let max_request = CreateVerifiableBeaconRequest {
        verifier_address: format!("{:?}", app_state.wallet_address),
        initial_data: u128::MAX,
        initial_cardinality: u32::MAX,
    };

    let max_result = create_verifiable_beacon(&app_state, max_request).await;
    match max_result {
        Ok(_) => println!("Maximum values succeeded"),
        Err(e) => println!("Maximum values failed: {}", e),
    }
}
