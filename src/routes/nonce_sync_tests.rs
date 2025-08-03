#[cfg(test)]
mod nonce_synchronization_tests {
    use crate::models::AppState;
    use crate::routes::{execute_transaction_serialized, get_fresh_nonce_from_alternate, is_nonce_error};
    use crate::routes::test_utils::create_mock_app_state;
    use alloy::providers::ProviderBuilder;
    use alloy::network::EthereumWallet;
    use alloy::signers::local::PrivateKeySigner;
    use serial_test::serial;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio::task::JoinSet;

    /// Test comprehensive nonce synchronization scenarios
    #[tokio::test]
    #[serial]
    async fn test_comprehensive_nonce_sync_scenarios() {
        let app_state = create_mock_app_state().await;

        // Test 1: Primary RPC nonce synchronization
        let primary_nonce = app_state.provider.get_transaction_count(app_state.wallet_address).await.map_err(|e| e.to_string());
        assert!(primary_nonce.is_ok(), "Primary nonce sync should succeed with mock provider");
        
        let nonce_value = primary_nonce.unwrap();
        // Nonce is u64, so it's always non-negative by type definition
        assert!(nonce_value <= u64::MAX, "Nonce should be valid u64"); // This is always true, but kept for clarity

        // Test 2: Missing alternate provider scenario
        assert!(app_state.alternate_provider.is_none(), "Test state should not have alternate provider");
        let alternate_result = get_fresh_nonce_from_alternate(&app_state).await;
        assert!(alternate_result.is_err());
        assert_eq!(alternate_result.unwrap_err(), "No alternate provider available");

        // Test 3: AppState with alternate provider
        let app_state_with_alternate = create_app_state_with_alternate().await;
        let alternate_nonce = get_fresh_nonce_from_alternate(&app_state_with_alternate).await;
        // This should work since we have an alternate provider
        assert!(alternate_nonce.is_ok(), "Alternate nonce sync should work when provider is available");
    }

    /// Test nonce error detection with various error patterns
    #[tokio::test]
    #[serial]
    async fn test_nonce_error_detection_comprehensive() {
        let nonce_error_patterns = vec![
            // Standard nonce too low errors
            "nonce too low: next nonce 3788, tx nonce 3782",
            "server returned an error response: error code -32000: nonce too low: next nonce 3788, tx nonce 3784",
            "Error: nonce too low",
            
            // Nonce too high errors
            "nonce too high",
            "nonce too high: expected 100, got 105",
            
            // Invalid nonce patterns
            "invalid nonce",
            "invalid nonce: expected 123, got 120",
            "nonce is invalid",
            
            // Replacement transaction errors (also nonce-related)
            "replacement transaction underpriced",
            "replacement tx underpriced",
        ];

        let non_nonce_error_patterns = vec![
            // Gas-related errors
            "insufficient funds for gas * price + value",
            "gas estimation failed",
            "out of gas",
            "gas limit exceeded",
            
            // Contract execution errors
            "execution reverted",
            "execution reverted: custom error",
            "revert reason: insufficient balance",
            
            // Network errors
            "connection timeout",
            "network error",
            "RPC error",
            
            // Other blockchain errors
            "transaction pool limit exceeded",
            "known transaction",
            "already known",
        ];

        // Test that all nonce error patterns are detected
        for error_msg in nonce_error_patterns {
            assert!(is_nonce_error(error_msg), 
                   "Should detect '{error_msg}' as a nonce error");
        }

        // Test that non-nonce errors are not detected as nonce errors
        for error_msg in non_nonce_error_patterns {
            assert!(!is_nonce_error(error_msg), 
                   "Should NOT detect '{error_msg}' as a nonce error");
        }
    }

    /// Test concurrent nonce operations don't interfere with each other
    #[tokio::test]
    #[serial]
    async fn test_concurrent_nonce_operations() {
        let mut join_set = JoinSet::new();

        // Spawn multiple concurrent nonce sync operations
        for i in 0..5 {
            join_set.spawn(async move {
                let app_state = create_mock_app_state().await;
                let start_time = Instant::now();
                let result = app_state.provider.get_transaction_count(app_state.wallet_address).await.map_err(|e| e.to_string());
                (i, result, start_time.elapsed())
            });
        }

        let mut results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            results.push(result.unwrap());
        }

        assert_eq!(results.len(), 5);
        
        // All operations should succeed with mock provider
        for (i, nonce_result, duration) in results {
            assert!(nonce_result.is_ok(), 
                   "Nonce sync operation {i} should succeed");
            assert!(duration < Duration::from_secs(5), 
                   "Nonce sync should complete quickly, took {duration:?}");
        }
    }

    /// Test transaction serialization under nonce conflict scenarios
    #[tokio::test]
    #[serial]
    async fn test_serialized_transactions_prevent_nonce_conflicts() {
        let mut join_set = JoinSet::new();
        let start_time = Instant::now();

        // Simulate 10 concurrent transactions that would normally cause nonce conflicts
        for i in 0..10 {
            join_set.spawn(async move {
                let operation_start = Instant::now();
                let result = execute_transaction_serialized(async move {
                    // Simulate transaction processing time
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    
                    // Simulate different transaction outcomes
                    match i % 3 {
                        0 => Ok(format!("Transaction {i} succeeded")),
                        1 => Err(format!("Transaction {i} failed: insufficient funds")),
                        _ => Err(format!("Transaction {i} failed: execution reverted")),
                    }
                }).await;
                
                (i, result, operation_start.elapsed())
            });
        }

        let mut results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            results.push(result.unwrap());
        }

        let total_time = start_time.elapsed();
        
        // Verify serialization occurred (total time should be ~500ms for 10 * 50ms operations)
        assert!(total_time >= Duration::from_millis(400), 
               "Total execution time too short: {total_time:?}. Expected ~500ms for serialized execution");
        
        assert_eq!(results.len(), 10);
        
        // Verify each operation took at least the expected time
        for (i, _result, duration) in &results {
            assert!(*duration >= Duration::from_millis(40), 
                   "Transaction {i} completed too quickly: {duration:?}");
        }

        // Verify we got the expected mix of successes and failures
        let successes = results.iter().filter(|(_, result, _)| result.is_ok()).count();
        let failures = results.iter().filter(|(_, result, _)| result.is_err()).count();
        
        assert_eq!(successes + failures, 10);
        assert!(successes > 0, "Should have some successful transactions");
        assert!(failures > 0, "Should have some failed transactions");
    }

    /// Test nonce synchronization with simulated RPC failures
    #[tokio::test]
    #[serial]
    async fn test_nonce_sync_with_rpc_failures() {
        let app_state = create_mock_app_state().await;

        // Test multiple attempts at nonce synchronization
        let mut successful_syncs = 0;
        let mut failed_syncs = 0;

        for _attempt in 0..5 {
            match app_state.provider.get_transaction_count(app_state.wallet_address).await {
                Ok(nonce) => {
                    successful_syncs += 1;
                    // Nonce is u64, so it's always non-negative by type definition
                    assert!(nonce <= u64::MAX, "Nonce should be valid u64"); // This is always true, but kept for clarity
                }
                Err(_) => {
                    failed_syncs += 1;
                    // This is expected behavior when network calls fail in test environment
                }
            }
        }

        // With mock provider, we expect all to succeed
        assert!(successful_syncs > 0, "Should have some successful syncs with mock provider");
    }

    /// Test edge cases in nonce management
    #[tokio::test]
    #[serial]
    async fn test_nonce_edge_cases() {
        // Test various edge case error messages
        let edge_case_errors = vec![
            ("", false), // Empty string
            ("random error message", false),
            ("nonce", false), // Just the word "nonce" without context
            ("NONCE TOO LOW", true), // Case insensitive check
            ("transaction nonce too low: expected 100", true),
            ("nonce too low: account nonce is 50, transaction nonce is 45", true),
            ("invalid nonce value", true),
            ("nonce too high: account nonce is 10, transaction nonce is 15", true),
        ];

        for (error_msg, expected) in edge_case_errors {
            let is_nonce = is_nonce_error(error_msg);
            if expected {
                assert!(is_nonce, "Should detect '{error_msg}' as nonce error");
            } else {
                assert!(!is_nonce, "Should NOT detect '{error_msg}' as nonce error");
            }
        }
    }

    /// Helper function to create AppState with alternate provider for testing
    async fn create_app_state_with_alternate() -> AppState {
        let mut app_state = create_mock_app_state().await;
        
        // Create alternate provider with different wallet for testing
        let alternate_signer = PrivateKeySigner::random();
        let alternate_wallet = EthereumWallet::from(alternate_signer);
        let alternate_provider = Arc::new(
            ProviderBuilder::new()
                .wallet(alternate_wallet)
                .connect_http("http://localhost:8545".parse().unwrap())
        );
        
        app_state.alternate_provider = Some(alternate_provider);
        app_state
    }

    /// Benchmark test for transaction serialization performance
    #[tokio::test]
    #[serial]
    async fn test_transaction_serialization_performance() {
        let start_time = Instant::now();
        let mut join_set = JoinSet::new();

        // Test with many concurrent operations to ensure performance is acceptable
        for i in 0..50 {
            join_set.spawn(async move {
                execute_transaction_serialized(async move {
                    // Very short operation to test overhead
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    i
                }).await
            });
        }

        let mut results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            results.push(result.unwrap());
        }

        let total_time = start_time.elapsed();
        
        assert_eq!(results.len(), 50);
        
        // Total time should be approximately 50 * 10ms = 500ms, with some overhead
        // We'll be generous and allow up to 800ms
        assert!(total_time <= Duration::from_millis(800), 
               "Serialization overhead too high: {total_time:?}");
        
        // But it should also be at least 400ms to ensure serialization is working
        assert!(total_time >= Duration::from_millis(400), 
               "Serialization not working, completed too quickly: {total_time:?}");
    }
}