//! Integration tests for transaction execution utilities
//!
//! Note: Transaction serialization is now handled by Redis-based distributed locks
//! in the wallet module (WalletLock). The global transaction serializer has been removed.
//! These tests cover nonce helper functions that are still relevant.

use alloy::primitives::U256;
use alloy::providers::Provider;
use serial_test::serial;

use the_beaconator::services::transaction::execution::{
    get_fresh_nonce_from_alternate, is_nonce_error,
};

/// Test fresh nonce retrieval from alternate provider
#[tokio::test]
#[serial]
#[ignore] // Temporarily disabled - hangs due to real network calls
async fn test_get_fresh_nonce_from_alternate_integration() {
    let (mut app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Test without alternate provider
    let result_no_alt = get_fresh_nonce_from_alternate(&app_state).await;
    assert!(
        result_no_alt.is_err(),
        "Should fail without alternate provider"
    );
    assert!(
        result_no_alt
            .unwrap_err()
            .contains("No alternate provider available")
    );

    // Set up alternate provider (use main provider for test)
    app_state.alternate_provider = Some(app_state.provider.clone());

    // Test with alternate provider
    let result_with_alt = get_fresh_nonce_from_alternate(&app_state).await;
    match result_with_alt {
        Ok(nonce) => {
            println!("Got fresh nonce from alternate provider: {nonce}");
            // Nonce should be reasonable (not too high)
            assert!(nonce < 1000000, "Nonce should be reasonable: {nonce}");
        }
        Err(e) => {
            // Might fail in test environment, but should not be "no provider" error
            println!("Alternate nonce fetch failed (may be expected): {e}");
            assert!(
                !e.contains("No alternate provider available"),
                "Should not be no provider error"
            );
        }
    }
}

/// Test nonce error detection with various error messages
#[tokio::test]
async fn test_nonce_error_detection_comprehensive() {
    let nonce_errors = vec![
        "nonce too low",
        "NONCE TOO LOW",
        "nonce too high",
        "invalid nonce",
        "nonce is invalid",
        "replacement transaction underpriced",
        "replacement tx underpriced",
        "Transaction nonce is too low. Try incrementing the nonce.",
        "err: nonce too low: address 0x123..., nonce: 5 current: 6",
        "invalid nonce: expected 10, got 8",
        "RPC Error: replacement transaction underpriced",
    ];

    for error_msg in nonce_errors {
        assert!(
            is_nonce_error(error_msg),
            "Should detect nonce error: {error_msg}"
        );
    }

    let non_nonce_errors = vec![
        "insufficient funds",
        "gas limit exceeded",
        "execution reverted",
        "network timeout",
        "connection refused",
        "invalid signature",
        "unknown error",
        "",
        "nonce",       // Just the word alone
        "replacement", // Just the word alone
    ];

    for error_msg in non_nonce_errors {
        assert!(
            !is_nonce_error(error_msg),
            "Should not detect nonce error: {error_msg}"
        );
    }
}

/// Test nonce synchronization with real network calls
#[tokio::test]
#[serial]
#[ignore] // Temporarily disabled - hangs due to real network calls
async fn test_nonce_synchronization_integration() {
    let (mut app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Set up alternate provider for nonce sync
    app_state.alternate_provider = Some(app_state.provider.clone());

    // Get current nonce
    let current_nonce = app_state
        .provider
        .get_transaction_count(app_state.wallet_address)
        .await;
    match current_nonce {
        Ok(nonce) => {
            println!("Current nonce: {nonce}");

            // Get fresh nonce from alternate
            let fresh_nonce = get_fresh_nonce_from_alternate(&app_state).await;
            match fresh_nonce {
                Ok(alt_nonce) => {
                    println!("Alternate nonce: {alt_nonce}");
                    // Should be the same or very close
                    let diff = alt_nonce.abs_diff(nonce);
                    assert!(
                        diff <= 10,
                        "Nonces should be close: current={nonce}, alt={alt_nonce}"
                    );
                }
                Err(e) => {
                    println!("Alternate nonce failed: {e}");
                    // Should not be "no provider" error
                    assert!(!e.contains("No alternate provider available"));
                }
            }
        }
        Err(e) => {
            println!("Failed to get current nonce: {e}");
        }
    }
}

/// Test basic network connectivity with Anvil
#[tokio::test]
#[serial]
#[ignore] // Temporarily disabled - hangs due to real network calls
async fn test_anvil_network_connectivity() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Check wallet balance
    let result = app_state
        .provider
        .get_balance(app_state.wallet_address)
        .await;

    assert!(result.is_ok(), "Balance check should succeed: {result:?}");

    if let Ok(balance) = result {
        println!("Wallet balance: {balance} wei");
        assert!(
            balance > U256::ZERO,
            "Wallet should have some ETH for tests"
        );
    }
}
