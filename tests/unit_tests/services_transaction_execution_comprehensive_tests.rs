//! Tests for transaction execution utilities
//!
//! Note: The execute_transaction_serialized function has been removed.
//! Transaction serialization is now handled by Redis-based distributed locks
//! in the wallet module. See WalletLock for details.

use alloy::primitives::Address;
use std::str::FromStr;
use the_beaconator::services::transaction::execution::{
    get_fresh_nonce_from_alternate, is_nonce_error,
};

#[tokio::test]
async fn test_get_fresh_nonce_from_alternate_no_provider() {
    let app_state = crate::test_utils::create_simple_test_app_state();
    // App state has no alternate provider by default

    let result = get_fresh_nonce_from_alternate(&app_state).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("No alternate provider available")
    );
}

#[tokio::test]
async fn test_get_fresh_nonce_from_alternate_with_provider() {
    let mut app_state = crate::test_utils::create_simple_test_app_state();

    // Add alternate provider (clone the main provider for testing)
    app_state.alternate_provider = Some(app_state.provider.clone());

    // Test that alternate provider is used (may succeed or fail depending on network)
    let result = get_fresh_nonce_from_alternate(&app_state).await;

    // Accept both Ok and Err, but if it errors, ensure it's not "No alternate provider available"
    match result {
        Ok(_nonce) => {
            // Success means alternate provider worked
        }
        Err(e) => {
            // If it errors, it should be a network/RPC error, not a missing provider error
            assert!(
                !e.contains("No alternate provider available"),
                "Should not error with missing provider when one is configured"
            );
        }
    }
}

#[test]
fn test_is_nonce_error_positive_cases() {
    let nonce_error_messages = vec![
        "nonce too low",
        "nonce too high",
        "invalid nonce",
        "nonce is invalid",
        "nonce is too low",
        "replacement transaction underpriced",
        "replacement tx underpriced",
        "NONCE TOO LOW",                    // Case insensitive
        "Invalid Nonce: Expected 5, got 3", // Mixed case
        "Transaction failed: nonce too low for account",
        "Error: replacement transaction underpriced",
        "RPC Error: nonce is invalid",
    ];

    for error_msg in nonce_error_messages {
        assert!(
            is_nonce_error(error_msg),
            "Should detect nonce error in: {error_msg}"
        );
    }
}

#[test]
fn test_is_nonce_error_negative_cases() {
    let non_nonce_error_messages = vec![
        "insufficient funds",
        "gas limit exceeded",
        "transaction reverted",
        "network timeout",
        "invalid signature",
        "contract execution failed",
        "unknown error",
        "", // Empty string
        "normal error message",
        "connection refused",
        "method not found",
    ];

    for error_msg in non_nonce_error_messages {
        assert!(
            !is_nonce_error(error_msg),
            "Should not detect nonce error in: {error_msg}"
        );
    }
}

#[test]
fn test_is_nonce_error_edge_cases() {
    // Test edge cases for nonce error detection
    assert!(!is_nonce_error("")); // Empty string
    assert!(is_nonce_error("nonce too low")); // Exact match
    assert!(is_nonce_error("  nonce too low  ")); // With whitespace
    assert!(is_nonce_error("Error: nonce too low. Please retry.")); // Embedded
    assert!(!is_nonce_error("noncetoolow")); // No spaces
    assert!(!is_nonce_error("since too low")); // Similar but different
}

#[test]
fn test_is_nonce_error_partial_matches() {
    // Test that partial matches work correctly
    assert!(is_nonce_error("The nonce too low error occurred"));
    assert!(is_nonce_error("Found invalid nonce in transaction"));
    assert!(is_nonce_error(
        "Replacement transaction underpriced, try again"
    ));
    assert!(!is_nonce_error("nonce")); // Just the word "nonce" alone
    assert!(!is_nonce_error("too low")); // Just "too low" without "nonce"
}

#[test]
fn test_is_nonce_error_all_variants() {
    // Test all specific nonce error patterns
    let patterns = vec![
        "nonce too low",
        "nonce too high",
        "invalid nonce",
        "nonce is invalid",
        "nonce is too low",
        "replacement transaction underpriced",
        "replacement tx underpriced",
    ];

    for pattern in patterns {
        // Test lowercase
        assert!(is_nonce_error(pattern));

        // Test uppercase
        assert!(is_nonce_error(&pattern.to_uppercase()));

        // Test with prefix and suffix
        let with_context = format!("Error occurred: {pattern} - please retry");
        assert!(is_nonce_error(&with_context));
    }
}

#[tokio::test]
async fn test_address_parsing_in_nonce_context() {
    // Test address parsing that might be used with nonce functions
    let app_state = crate::test_utils::create_simple_test_app_state();

    // Verify wallet address is valid (use Display for canonical hex format)
    let wallet_addr_str = app_state.wallet_address.to_string();
    assert!(wallet_addr_str.starts_with("0x"));
    assert_eq!(wallet_addr_str.len(), 42); // 0x + 40 hex chars

    // Test parsing the same address
    let parsed = Address::from_str(&wallet_addr_str);
    assert!(parsed.is_ok());
    assert_eq!(parsed.unwrap(), app_state.wallet_address);
}

#[test]
fn test_nonce_error_message_generation() {
    // Test generating error messages that would be caught by is_nonce_error
    let nonce_value = 42u64;

    let error_msg = format!(
        "nonce too low: expected {}, got {}",
        nonce_value + 1,
        nonce_value
    );
    assert!(is_nonce_error(&error_msg));

    let error_msg2 = format!("invalid nonce {nonce_value}");
    assert!(is_nonce_error(&error_msg2));

    let error_msg3 = format!("replacement transaction underpriced for nonce {nonce_value}");
    assert!(is_nonce_error(&error_msg3));
}
