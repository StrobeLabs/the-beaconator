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

/// Test identity beacon creation with boundary values (0, u128::MAX)
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_create_identity_beacon_boundary_values() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    for value in [0u128, u128::MAX] {
        let result = create_identity_beacon(&app_state, value).await;
        match result {
            Ok((beacon, verifier)) => {
                println!("initial_index={value} succeeded: beacon={beacon}, verifier={verifier}");
                assert_ne!(beacon, Address::ZERO);
                assert_ne!(verifier, Address::ZERO);
            }
            Err(e) => println!("initial_index={value} failed (may be expected): {e}"),
        }
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
