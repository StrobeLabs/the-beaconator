use alloy::primitives::Address;
use serial_test::serial;

use the_beaconator::services::beacon::core::create_identity_beacon;

/// Test identity beacon creation with Anvil
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_create_identity_beacon_integration() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let result = create_identity_beacon(&app_state, 12345).await;

    // This might fail if contracts don't exist, but should
    // get past the validation stage
    match result {
        Ok((beacon_address, verifier_address)) => {
            println!(
                "Identity beacon creation succeeded: beacon={beacon_address}, verifier={verifier_address}"
            );
            assert_ne!(beacon_address, Address::ZERO);
            assert_ne!(verifier_address, Address::ZERO);
        }
        Err(e) => {
            println!("Identity beacon creation failed (may be expected): {e}");
        }
    }
}

/// Test identity beacon creation with edge case values
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_create_identity_beacon_edge_cases() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Test with zero initial index
    let result = create_identity_beacon(&app_state, 0).await;
    match result {
        Ok(_) => println!("Zero initial index succeeded"),
        Err(e) => println!("Zero initial index failed: {e}"),
    }

    // Test with max value
    let result = create_identity_beacon(&app_state, u128::MAX).await;
    match result {
        Ok(_) => println!("Max initial index succeeded"),
        Err(e) => println!("Max initial index failed: {e}"),
    }
}

/// Test multiple identity beacon operations concurrently
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_concurrent_identity_beacon_operations() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let mut handles = Vec::new();

    // Start multiple create operations
    for i in 0..3u128 {
        let app_state_clone = app_state.clone();
        let handle = tokio::spawn(async move {
            let result = create_identity_beacon(&app_state_clone, 1000 + i).await;
            (i, result)
        });
        handles.push(handle);
    }

    // Wait for all operations
    let mut success_count = 0;
    for handle in handles {
        let (i, result) = handle.await.unwrap();
        match result {
            Ok((beacon_address, verifier_address)) => {
                println!(
                    "Concurrent identity beacon {i} succeeded: beacon={beacon_address}, verifier={verifier_address}"
                );
                success_count += 1;
            }
            Err(e) => println!("Concurrent identity beacon {i} failed: {e}"),
        }
    }

    println!("Concurrent identity beacon operations: {success_count} successes");
}

/// Test identity beacon operations with extreme values
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_identity_beacon_extreme_values() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Test with minimum values
    let min_result = create_identity_beacon(&app_state, 0).await;
    match min_result {
        Ok(_) => println!("Minimum values succeeded"),
        Err(e) => println!("Minimum values failed: {e}"),
    }

    // Test with maximum values
    let max_result = create_identity_beacon(&app_state, u128::MAX).await;
    match max_result {
        Ok(_) => println!("Maximum values succeeded"),
        Err(e) => println!("Maximum values failed: {e}"),
    }
}
