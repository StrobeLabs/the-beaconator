// Transaction execution tests - extracted from src/services/transaction/execution.rs

use the_beaconator::services::transaction::execution::{get_transaction_mutex, is_nonce_error};

#[test]
fn test_is_nonce_error_detection() {
    // Test various nonce error patterns
    assert!(is_nonce_error("nonce too low"));
    assert!(is_nonce_error("NONCE TOO LOW")); // Case insensitive
    assert!(is_nonce_error("Error: nonce too high"));
    assert!(is_nonce_error("invalid nonce"));
    assert!(is_nonce_error("replacement transaction underpriced"));

    // Non-nonce errors should return false
    assert!(!is_nonce_error("insufficient funds"));
    assert!(!is_nonce_error("gas limit exceeded"));
    assert!(!is_nonce_error(""));
}

#[test]
fn test_transaction_mutex_initialization() {
    // Test that we can get the transaction mutex (it should exist)
    let mutex = get_transaction_mutex();

    // Just verify the mutex exists and we can reference it
    // We don't try to lock it since other tests might be using it
    // The fact that this compiles and runs means the mutex is properly initialized
    assert_eq!(
        std::ptr::addr_of!(*mutex),
        std::ptr::addr_of!(*get_transaction_mutex())
    );
}
