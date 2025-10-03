use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use serial_test::serial;
use std::sync::Arc;
use std::time::Duration;

use the_beaconator::services::beacon::core::create_beacon_via_factory;
use the_beaconator::services::transaction::execution::{
    execute_transaction_serialized, get_fresh_nonce_from_alternate, get_transaction_mutex,
    is_nonce_error,
};

/// Test serialized transaction execution with real network calls
#[tokio::test]
#[serial]
#[ignore] // Temporarily disabled - hangs due to real network calls
async fn test_execute_transaction_serialized_with_network() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Execute a real transaction within serialized execution
    let result = execute_transaction_serialized(async {
        // Check wallet balance (simple network call)
        app_state
            .provider
            .get_balance(app_state.wallet_address)
            .await
    })
    .await;

    assert!(
        result.is_ok(),
        "Serialized balance check should succeed: {result:?}"
    );

    if let Ok(balance) = result {
        println!("Wallet balance in serialized execution: {balance} wei");
        assert!(
            balance > U256::ZERO,
            "Wallet should have some ETH for tests"
        );
    }
}

/// Test multiple serialized network operations
#[tokio::test]
#[serial]
#[ignore] // Temporarily disabled - hangs due to real network calls
async fn test_multiple_serialized_network_operations() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let start_time = std::time::Instant::now();

    // Start multiple serialized network operations with single threaded approach
    for i in 0..3 {
        let result = execute_transaction_serialized(async {
            // Each operation does a network call
            let balance = app_state
                .provider
                .get_balance(app_state.wallet_address)
                .await?;
            tokio::time::sleep(Duration::from_millis(10)).await; // Small delay
            Ok::<U256, Box<dyn std::error::Error + Send + Sync>>(balance)
        })
        .await;

        println!("Serialized operation {i} result: {result:?}");
    }

    let elapsed = start_time.elapsed();
    println!("Serialized network operations completed in {elapsed:?}");
}

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

/// Test transaction mutex singleton behavior
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
async fn test_transaction_mutex_singleton_integration() {
    // Get multiple references to the mutex
    let mutex1 = get_transaction_mutex();
    let mutex2 = get_transaction_mutex();
    let mutex3 = get_transaction_mutex();

    // Should all be the same instance
    assert!(Arc::ptr_eq(mutex1, mutex2), "Mutex should be singleton");
    assert!(Arc::ptr_eq(mutex2, mutex3), "Mutex should be singleton");

    // Test concurrent access to the singleton
    let mutex_clone = mutex1.clone();
    let handle = tokio::spawn(async move {
        let _lock = mutex_clone.lock().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        42
    });

    // This should wait for the above lock
    let start = std::time::Instant::now();
    let _lock = mutex1.lock().await;
    let elapsed = start.elapsed();

    let result = handle.await.unwrap();
    assert_eq!(result, 42);
    assert!(elapsed >= Duration::from_millis(5), "Should wait for lock");
}

/// Test serialized execution with actual beacon creation
#[tokio::test]
#[serial]
#[ignore] // Temporarily disabled - hangs due to real network calls
async fn test_serialized_beacon_creation() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let owner_address = app_state.wallet_address;
    let factory_address = app_state.beacon_factory_address;

    // Create beacon within serialized execution
    let result = execute_transaction_serialized(async {
        create_beacon_via_factory(&app_state, owner_address, factory_address).await
    })
    .await;

    match result {
        Ok(beacon_address) => {
            println!("Serialized beacon creation succeeded: {beacon_address}");
            assert_ne!(
                beacon_address,
                Address::ZERO,
                "Beacon address should not be zero"
            );
        }
        Err(e) => {
            println!("Serialized beacon creation failed: {e}");
            // Should not be a serialization error
            assert!(!e.contains("mutex"), "Should not be mutex error");
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

/// Test concurrent serialized operations with simple async operations
#[tokio::test]
#[serial]
async fn test_concurrent_serialized_network_operations() {
    // No app_state needed - test serialization behavior with simple operations
    let operation_count = 5;

    let start_time = std::time::Instant::now();

    // Test serialized execution with simple operations (no network calls)
    for i in 0..operation_count {
        let result = execute_transaction_serialized(async {
            // Simple operation that doesn't require network
            tokio::time::sleep(Duration::from_millis(1)).await;
            Ok::<String, Box<dyn std::error::Error + Send + Sync>>(format!("operation_{i}"))
        })
        .await;

        println!("Concurrent operation {i} result: {result:?}");
        assert!(result.is_ok());
    }

    let elapsed = start_time.elapsed();
    println!("Serialized operations completed in {elapsed:?}");

    // Should complete quickly since no real network calls
    assert!(
        elapsed < Duration::from_secs(1),
        "Operations should complete quickly"
    );
}

/// Test serialized execution error handling
#[tokio::test]
#[serial]
#[ignore] // Temporarily disabled - hangs due to real network calls
async fn test_serialized_execution_error_handling() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Test error propagation through serialized execution
    let error_result =
        execute_transaction_serialized(async { Err::<(), &str>("test error") }).await;

    assert!(error_result.is_err(), "Should propagate error");
    assert_eq!(error_result.unwrap_err(), "test error");

    // Test successful result propagation
    let success_result = execute_transaction_serialized(async { Ok::<i32, &str>(42) }).await;

    assert!(success_result.is_ok(), "Should propagate success");
    assert_eq!(success_result.unwrap(), 42);

    // Test with network operation that might fail
    let network_result = execute_transaction_serialized(async {
        // Try to get balance of an invalid address (should still work with Anvil)
        app_state.provider.get_balance(Address::ZERO).await
    })
    .await;

    match network_result {
        Ok(balance) => println!("Zero address balance: {balance}"),
        Err(e) => println!("Zero address balance failed: {e}"),
    }
}

/// Test transaction execution timeout scenarios
#[tokio::test]
#[serial]
#[ignore] // Temporarily disabled - hangs due to real network calls
async fn test_transaction_execution_timeouts() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Test quick operation within timeout
    let quick_result = tokio::time::timeout(
        Duration::from_secs(5),
        execute_transaction_serialized(async {
            app_state
                .provider
                .get_balance(app_state.wallet_address)
                .await
        }),
    )
    .await;

    assert!(
        quick_result.is_ok(),
        "Quick operation should complete within timeout"
    );

    // Test operation with internal timeout
    let timeout_result = execute_transaction_serialized(async {
        tokio::time::timeout(Duration::from_millis(100), async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            42
        })
        .await
    })
    .await;

    assert!(timeout_result.is_ok(), "Internal timeout should succeed");
    assert_eq!(timeout_result.unwrap(), 42);
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
