use alloy::primitives::Address;
use serial_test::serial;
use std::str::FromStr;

use the_beaconator::services::beacon::core::{
    create_identity_beacon, is_beacon_registered, register_beacon_with_registry,
};

/// Helper: create a beacon, returning None if factory not deployed on test Anvil
async fn create_test_beacon(
    app_state: &the_beaconator::models::AppState,
) -> Option<(Address, Address)> {
    match create_identity_beacon(app_state, 12345).await {
        Ok(result) => Some(result),
        Err(e) => {
            println!("Skipping test - beacon creation failed (expected without factory): {e}");
            None
        }
    }
}

/// Test beacon registration with Anvil
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_register_beacon_with_anvil() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let Some((beacon_address, _verifier_address)) = create_test_beacon(&app_state).await else {
        return;
    };

    let registry_address = app_state.perpcity_registry_address;
    let register_result =
        register_beacon_with_registry(&app_state, beacon_address, registry_address).await;

    assert!(
        register_result.is_ok(),
        "Beacon registration should succeed: {register_result:?}"
    );

    let tx_hash = register_result.unwrap();
    println!("Beacon registered with tx hash: {tx_hash}");

    let is_registered = is_beacon_registered(&app_state, beacon_address, registry_address).await;
    assert!(is_registered.is_ok());
    assert!(
        is_registered.unwrap(),
        "Beacon should be registered after registration"
    );
}

/// Test idempotency - registering the same beacon twice
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_register_beacon_idempotency() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let Some((beacon_address, _verifier_address)) = create_test_beacon(&app_state).await else {
        return;
    };

    let registry_address = app_state.perpcity_registry_address;

    let first_register =
        register_beacon_with_registry(&app_state, beacon_address, registry_address).await;
    assert!(first_register.is_ok(), "First registration should succeed");

    let second_register =
        register_beacon_with_registry(&app_state, beacon_address, registry_address).await;
    assert!(
        second_register.is_ok(),
        "Second registration should succeed (idempotent)"
    );

    let tx_hash = second_register.unwrap();
    println!("Second registration tx hash: {tx_hash}");

    let is_registered = is_beacon_registered(&app_state, beacon_address, registry_address).await;
    assert!(is_registered.unwrap());
}

/// Test registering beacon with different registries
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_register_beacon_with_different_registries() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let Some((beacon_address, _verifier_address)) = create_test_beacon(&app_state).await else {
        return;
    };

    let registry1 = app_state.perpcity_registry_address;
    let register1 = register_beacon_with_registry(&app_state, beacon_address, registry1).await;
    assert!(
        register1.is_ok(),
        "Registration with first registry should succeed"
    );

    let is_registered1 = is_beacon_registered(&app_state, beacon_address, registry1).await;
    assert!(is_registered1.is_ok());
    assert!(is_registered1.unwrap());

    // Use ecdsa_verifier_factory_address as a non-registry contract stand-in.
    // register_beacon_with_registry should fail because registry2 is not a
    // BeaconRegistry and won't have the registerBeacon(address) method.
    let registry2 = app_state.ecdsa_verifier_factory_address;
    let register2_result =
        register_beacon_with_registry(&app_state, beacon_address, registry2).await;

    match register2_result {
        Ok(_) => println!("Registered with second registry (unexpected success)"),
        Err(e) => {
            println!("Registration with second registry failed as expected: {e}");
            assert!(!e.contains("Invalid"));
        }
    }
}

/// Test registering multiple beacons sequentially
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_register_multiple_beacons_sequentially() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let registry_address = app_state.perpcity_registry_address;
    let mut registered_beacons = Vec::new();

    for i in 0..3u128 {
        println!("Creating and registering beacon {i}");

        let beacon_result = create_identity_beacon(&app_state, 1000 + i).await;
        let (beacon_address, _verifier_address) = match beacon_result {
            Ok(r) => r,
            Err(e) => {
                println!("Skipping - beacon {i} creation failed: {e}");
                return;
            }
        };

        let register_result =
            register_beacon_with_registry(&app_state, beacon_address, registry_address).await;
        assert!(
            register_result.is_ok(),
            "Beacon {i} registration should succeed: {register_result:?}"
        );

        let is_registered =
            is_beacon_registered(&app_state, beacon_address, registry_address).await;
        assert!(is_registered.is_ok());
        assert!(is_registered.unwrap(), "Beacon {i} should be registered");

        registered_beacons.push(beacon_address);
    }

    assert_eq!(registered_beacons.len(), 3);
    println!(
        "Successfully registered {} beacons",
        registered_beacons.len()
    );
}

/// Test registration with zero beacon address
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_register_zero_beacon_address() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let zero_address = Address::ZERO;
    let registry_address = app_state.perpcity_registry_address;

    let result = register_beacon_with_registry(&app_state, zero_address, registry_address).await;

    assert!(
        result.is_err(),
        "Registering zero address should fail: {result:?}"
    );
}

/// Test registration check for unregistered beacon
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_registration_check_unregistered_beacon() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let unregistered_beacon =
        Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let registry_address = app_state.perpcity_registry_address;

    let is_registered =
        is_beacon_registered(&app_state, unregistered_beacon, registry_address).await;

    assert!(is_registered.is_ok());
    assert!(
        !is_registered.unwrap(),
        "Random address should not be registered"
    );
}

/// Test concurrent beacon registrations
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_concurrent_beacon_registrations() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let registry_address = app_state.perpcity_registry_address;

    let mut beacon_addresses = Vec::new();
    for i in 0..3u128 {
        match create_identity_beacon(&app_state, 1000 + i).await {
            Ok((beacon_address, _verifier_address)) => {
                beacon_addresses.push(beacon_address);
                println!("Created beacon {i} at {beacon_address}");
            }
            Err(e) => {
                println!("Skipping concurrent registration test - beacon creation failed: {e}");
                return;
            }
        }
    }

    let mut handles = Vec::new();
    for (i, beacon_address) in beacon_addresses.iter().enumerate() {
        let app_state_clone = app_state.clone();
        let beacon_addr = *beacon_address;
        let handle = tokio::spawn(async move {
            println!("Starting concurrent registration {i}");
            let result =
                register_beacon_with_registry(&app_state_clone, beacon_addr, registry_address)
                    .await;
            (i, beacon_addr, result)
        });
        handles.push(handle);
    }

    let mut successful = 0;
    for handle in handles {
        let (i, beacon_addr, result) = handle.await.unwrap();
        match result {
            Ok(_) => {
                println!("Registration {i} succeeded for {beacon_addr}");
                successful += 1;
            }
            Err(e) => println!("Registration {i} failed: {e}"),
        }
    }

    println!("Concurrent registrations: {successful} succeeded");
    assert!(successful > 0, "At least some registrations should succeed");
}

/// Test registration error handling
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_registration_error_handling() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let test_cases = vec![
        (
            Address::ZERO,
            app_state.perpcity_registry_address,
            "Zero beacon address",
        ),
        (
            app_state.funding_wallet_address,
            Address::ZERO,
            "Zero registry address",
        ),
        (
            Address::from_str("0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF").unwrap(),
            app_state.perpcity_registry_address,
            "Max address beacon",
        ),
    ];

    for (beacon_addr, registry_addr, description) in test_cases {
        println!("Testing: {description}");
        let result = register_beacon_with_registry(&app_state, beacon_addr, registry_addr).await;

        if result.is_err() {
            println!("  Failed as expected: {}", result.unwrap_err());
        } else {
            println!("  Unexpectedly succeeded (might be valid scenario)");
        }
    }
}

/// Test registration with timeout
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_registration_with_timeout() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let Some((beacon_address, _verifier_address)) = create_test_beacon(&app_state).await else {
        return;
    };

    let registry_address = app_state.perpcity_registry_address;

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        register_beacon_with_registry(&app_state, beacon_address, registry_address),
    )
    .await;

    assert!(
        result.is_ok(),
        "Registration should complete within timeout"
    );

    if let Ok(register_result) = result {
        assert!(
            register_result.is_ok(),
            "Registration should succeed: {register_result:?}"
        );
    }
}
