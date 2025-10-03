use alloy::primitives::Address;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use the_beaconator::services::transaction::execution::{
    execute_transaction_serialized, get_fresh_nonce_from_alternate, get_transaction_mutex,
    is_nonce_error,
};
use tokio::sync::Mutex;

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

    // This will fail in test environment due to network issues, which is expected
    let result = get_fresh_nonce_from_alternate(&app_state).await;
    assert!(result.is_err());
    // Should get a network error, not "No alternate provider available"
    assert!(
        !result
            .unwrap_err()
            .contains("No alternate provider available")
    );
}

#[test]
fn test_get_transaction_mutex_singleton() {
    // Test that the mutex is a singleton
    let mutex1 = get_transaction_mutex();
    let mutex2 = get_transaction_mutex();

    // Should be the same instance
    assert!(Arc::ptr_eq(mutex1, mutex2));
}

#[tokio::test]
async fn test_execute_transaction_serialized_simple() {
    // Test with a simple async operation
    let result = execute_transaction_serialized(async {
        tokio::time::sleep(Duration::from_millis(1)).await;
        42
    })
    .await;

    assert_eq!(result, 42);
}

#[tokio::test]
async fn test_execute_transaction_serialized_error() {
    // Test with an operation that returns an error
    let result: Result<i32, &str> =
        execute_transaction_serialized(async { Err("test error") }).await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "test error");
}

#[tokio::test]
async fn test_execute_transaction_serialized_multiple_concurrent() {
    // Test that multiple concurrent operations are properly serialized
    let start_time = std::time::Instant::now();

    let handles: Vec<_> = (0..5)
        .map(|i| {
            tokio::spawn(async move {
                execute_transaction_serialized(async move {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    i
                })
                .await
            })
        })
        .collect();

    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap());
    }

    let elapsed = start_time.elapsed();

    // Results should contain all values
    results.sort();
    assert_eq!(results, vec![0, 1, 2, 3, 4]);

    // Should take at least 50ms since operations are serialized
    assert!(elapsed >= Duration::from_millis(40));
}

#[tokio::test]
async fn test_execute_transaction_serialized_with_string_result() {
    let result = execute_transaction_serialized(async { "test_result".to_string() }).await;

    assert_eq!(result, "test_result");
}

#[tokio::test]
async fn test_execute_transaction_serialized_with_complex_type() {
    #[derive(Debug, PartialEq)]
    struct TestStruct {
        id: u32,
        name: String,
    }

    let result = execute_transaction_serialized(async {
        TestStruct {
            id: 123,
            name: "test".to_string(),
        }
    })
    .await;

    assert_eq!(result.id, 123);
    assert_eq!(result.name, "test");
}

#[test]
fn test_is_nonce_error_positive_cases() {
    let nonce_error_messages = vec![
        "nonce too low",
        "nonce too high",
        "invalid nonce",
        "nonce is invalid",
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
async fn test_get_transaction_mutex_concurrent_access() {
    // Test that the mutex properly serializes access
    let mutex = get_transaction_mutex();
    let counter = Arc::new(Mutex::new(0));

    let handles: Vec<_> = (0..10)
        .map(|_| {
            let mutex = mutex.clone();
            let counter = counter.clone();
            tokio::spawn(async move {
                let _lock = mutex.lock().await;
                let mut c = counter.lock().await;
                let old_value = *c;
                tokio::time::sleep(Duration::from_millis(1)).await; // Simulate work
                *c = old_value + 1;
            })
        })
        .collect();

    for handle in handles {
        handle.await.unwrap();
    }

    let final_count = *counter.lock().await;
    assert_eq!(final_count, 10); // All increments should have happened
}

#[tokio::test]
async fn test_execute_transaction_serialized_with_future_combinator() {
    // Test with various future combinators
    let result = execute_transaction_serialized(async {
        let future1 = async { 10 };
        let future2 = async { 20 };
        let (a, b) = tokio::join!(future1, future2);
        a + b
    })
    .await;

    assert_eq!(result, 30);
}

#[tokio::test]
async fn test_execute_transaction_serialized_timeout() {
    // Test that timeouts work within serialized execution
    let result = execute_transaction_serialized(async {
        match tokio::time::timeout(Duration::from_millis(10), async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            42
        })
        .await
        {
            Ok(value) => Ok(value),
            Err(_) => Err("timeout"),
        }
    })
    .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "timeout");
}

#[test]
fn test_mutex_type_safety() {
    // Test that the mutex has the correct type
    let mutex = get_transaction_mutex();

    // Should be Arc<Mutex<()>>
    let _: &Arc<Mutex<()>> = mutex;

    // Test that it can be cloned
    let _cloned = mutex.clone();
}

#[tokio::test]
async fn test_address_parsing_in_nonce_context() {
    // Test address parsing that might be used with nonce functions
    let app_state = crate::test_utils::create_simple_test_app_state();

    // Verify wallet address is valid
    let wallet_addr_str = format!("{:?}", app_state.wallet_address);
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

    let error_msg3 = format!(
        "replacement transaction underpriced for nonce {nonce_value}"
    );
    assert!(is_nonce_error(&error_msg3));
}

#[tokio::test]
async fn test_serialized_execution_ordering() {
    // Test that operations execute in submission order when serialized
    let results = Arc::new(Mutex::new(Vec::new()));

    let handles: Vec<_> = (0..5)
        .map(|i| {
            let results = results.clone();
            tokio::spawn(async move {
                execute_transaction_serialized(async move {
                    // Add small delay to ensure ordering matters
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    let mut r = results.lock().await;
                    r.push(i);
                })
                .await
            })
        })
        .collect();

    for handle in handles {
        handle.await.unwrap();
    }

    let final_results = results.lock().await;
    assert_eq!(final_results.len(), 5);
    // Results should be in order (serialized execution)
    for i in 0..4 {
        assert!(final_results[i] <= final_results[i + 1]);
    }
}
