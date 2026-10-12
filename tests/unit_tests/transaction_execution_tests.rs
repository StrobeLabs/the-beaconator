// Transaction execution tests - extracted from src/services/transaction/execution.rs
//
// Note: The global transaction serializer has been removed.
// Transaction serialization is now handled by Redis-based distributed locks
// in the wallet module. See `WalletLock` for details.

use the_beaconator::services::transaction::execution::{
    is_insufficient_funds_error, is_nonce_error,
};

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
fn test_is_insufficient_funds_error_detection() {
    // Test various insufficient-funds error patterns
    assert!(is_insufficient_funds_error("insufficient funds"));
    assert!(is_insufficient_funds_error("INSUFFICIENT FUNDS")); // Case insensitive
    assert!(is_insufficient_funds_error(
        "Error: insufficient funds for gas * price + value"
    ));
    assert!(is_insufficient_funds_error(
        "insufficient balance for transfer"
    ));
    assert!(is_insufficient_funds_error(
        "gas required exceeds allowance"
    ));

    // Non insufficient-funds errors should return false
    assert!(!is_insufficient_funds_error("nonce too low"));
    assert!(!is_insufficient_funds_error("gas limit exceeded"));
    assert!(!is_insufficient_funds_error(""));
}

mod send_with_nonce_retry_tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use the_beaconator::services::transaction::execution::send_with_nonce_retry;

    #[tokio::test]
    async fn retries_nonce_error_then_succeeds() {
        let attempts = AtomicUsize::new(0);
        let result = send_with_nonce_retry("test-op", || {
            let n = attempts.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                if n == 0 {
                    // The exact shape observed on prod 2026-08-24.
                    Err("server returned an error response: error code -32000: \
                         nonce too low: address 0xB5C0..., tx: 9055 state: 9056"
                        .to_string())
                } else {
                    Ok(42u32)
                }
            })
        })
        .await;
        assert_eq!(result, Ok(42));
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn non_nonce_error_is_not_retried() {
        let attempts = AtomicUsize::new(0);
        let result: Result<u32, String> = send_with_nonce_retry("test-op", || {
            attempts.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Err("insufficient funds for gas * price + value".to_string()) })
        })
        .await;
        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn persistent_nonce_error_exhausts_and_returns_last_error() {
        let attempts = AtomicUsize::new(0);
        let result: Result<u32, String> = send_with_nonce_retry("test-op", || {
            attempts.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Err("nonce too low: tx 7 state 9".to_string()) })
        })
        .await;
        assert_eq!(result, Err("nonce too low: tx 7 state 9".to_string()));
        // Initial attempt + one retry per backoff step.
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn first_attempt_success_sends_once() {
        let attempts = AtomicUsize::new(0);
        let result = send_with_nonce_retry("test-op", || {
            attempts.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok("ok") })
        })
        .await;
        assert_eq!(result, Ok("ok"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
