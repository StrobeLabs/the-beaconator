use alloy::primitives::Address;
use serial_test::serial;
use std::str::FromStr;

use the_beaconator::services::beacon::core::{
    create_identity_beacon, is_beacon_registered, register_beacon_with_registry,
    unregister_beacon_with_registry,
};

// These integration tests mirror the register-beacon integration tests and are `#[ignore]`d for
// the same reason: the shared Anvil harness does not deploy the beacon factories, so beacon
// creation no-ops and the on-chain calls hang against a real RPC. They document intended behavior
// and can be run manually (`--ignored`) once a factory-seeded Anvil fixture exists.

/// Helper: create a beacon, returning None if the factory is not deployed on the test Anvil.
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

/// Register then unregister a beacon; it should end up not registered.
#[tokio::test]
#[ignore] // Disabled - hangs due to real network calls (mirrors register integration tests)
#[serial]
async fn test_unregister_beacon_with_anvil() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let Some((beacon_address, _verifier_address)) = create_test_beacon(&app_state).await else {
        return;
    };

    let registry_address = app_state.contracts.perpcity_registry;

    let register_result =
        register_beacon_with_registry(&app_state, beacon_address, registry_address).await;
    assert!(
        register_result.is_ok(),
        "Beacon registration should succeed: {register_result:?}"
    );
    assert!(
        is_beacon_registered(&app_state, beacon_address, registry_address)
            .await
            .unwrap(),
        "Beacon should be registered before unregistration"
    );

    let unregister_result =
        unregister_beacon_with_registry(&app_state, beacon_address, registry_address).await;
    assert!(
        unregister_result.is_ok(),
        "Beacon unregistration should succeed: {unregister_result:?}"
    );
    println!(
        "Beacon unregistered with outcome: {:?}",
        unregister_result.unwrap()
    );

    assert!(
        !is_beacon_registered(&app_state, beacon_address, registry_address)
            .await
            .unwrap(),
        "Beacon should NOT be registered after unregistration"
    );
}

/// Unregistering a beacon that was never registered is a no-op success.
#[tokio::test]
#[ignore] // Disabled - hangs due to real network calls (mirrors register integration tests)
#[serial]
async fn test_unregister_unregistered_beacon_is_noop() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let never_registered = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let registry_address = app_state.contracts.perpcity_registry;

    // Precondition: it is not registered.
    assert!(
        !is_beacon_registered(&app_state, never_registered, registry_address)
            .await
            .unwrap()
    );

    let result =
        unregister_beacon_with_registry(&app_state, never_registered, registry_address).await;
    assert!(
        result.is_ok(),
        "Unregistering a never-registered beacon should be a no-op success: {result:?}"
    );
}

/// Unregistering twice is idempotent (second call is a no-op success).
#[tokio::test]
#[ignore] // Disabled - hangs due to real network calls (mirrors register integration tests)
#[serial]
async fn test_unregister_beacon_idempotency() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let Some((beacon_address, _verifier_address)) = create_test_beacon(&app_state).await else {
        return;
    };

    let registry_address = app_state.contracts.perpcity_registry;

    assert!(
        register_beacon_with_registry(&app_state, beacon_address, registry_address)
            .await
            .is_ok()
    );

    let first = unregister_beacon_with_registry(&app_state, beacon_address, registry_address).await;
    assert!(first.is_ok(), "First unregistration should succeed");

    let second =
        unregister_beacon_with_registry(&app_state, beacon_address, registry_address).await;
    assert!(
        second.is_ok(),
        "Second unregistration should succeed (idempotent no-op)"
    );

    assert!(
        !is_beacon_registered(&app_state, beacon_address, registry_address)
            .await
            .unwrap()
    );
}
