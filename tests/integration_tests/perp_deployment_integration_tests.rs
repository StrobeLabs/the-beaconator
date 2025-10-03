use alloy::primitives::{Address, U256};
use serial_test::serial;
use std::str::FromStr;

use the_beaconator::models::DepositLiquidityForPerpRequest;
use the_beaconator::services::beacon::core::create_beacon_via_factory;
use the_beaconator::services::perp::operations::{
    deploy_perp_for_beacon, deposit_liquidity_for_perp,
};

/// Test perp deployment for beacon with Anvil
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_deploy_perp_for_beacon_integration() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // First create a beacon to deploy perp for
    let owner_address = app_state.wallet_address;
    let factory_address = app_state.beacon_factory_address;

    let beacon_result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;
    assert!(
        beacon_result.is_ok(),
        "Beacon creation should succeed for perp test"
    );

    let beacon_address = beacon_result.unwrap();
    println!("Created beacon for perp deployment: {beacon_address}");

    // Deploy perp for the beacon
    let deploy_result = deploy_perp_for_beacon(&app_state, beacon_address).await;

    match deploy_result {
        Ok(response) => {
            println!("Perp deployment succeeded:");
            println!("  Transaction hash: {}", response.transaction_hash);
            println!("  Perp ID: {}", response.perp_id);
            assert!(
                response.transaction_hash.starts_with("0x"),
                "Should have valid transaction hash"
            );
            assert!(
                response.perp_id.starts_with("0x"),
                "Should have valid perp id"
            );
        }
        Err(e) => {
            println!("Perp deployment failed (may be expected with test contracts): {e}");
            // Should get to the contract interaction stage, not fail on validation
            assert!(
                !e.contains("Beacon address cannot be zero"),
                "Should not be validation error"
            );
            assert!(
                !e.contains("Invalid beacon address"),
                "Should not be address parsing error"
            );
        }
    }
}

/// Test perp deployment with zero beacon address
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_deploy_perp_zero_beacon_address() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let zero_address = Address::ZERO;

    let result = deploy_perp_for_beacon(&app_state, zero_address).await;

    // Should handle zero address gracefully
    match result {
        Ok(_) => println!("Zero beacon address deployment unexpectedly succeeded"),
        Err(e) => {
            println!("Zero beacon address deployment failed as expected: {e}");
            // Could fail at validation or contract level
        }
    }
}

/// Test perp deployment with invalid beacon (no code)
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_deploy_perp_invalid_beacon() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Use a random address that has no code deployed
    let invalid_beacon = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();

    let result = deploy_perp_for_beacon(&app_state, invalid_beacon).await;

    match result {
        Ok(_) => println!("Invalid beacon deployment unexpectedly succeeded"),
        Err(e) => {
            println!("Invalid beacon deployment failed: {e}");
            // Should detect that beacon has no code or fail at contract level
        }
    }
}

/// Test multiple perp deployments for different beacons
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_multiple_perp_deployments() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let owner_address = app_state.wallet_address;
    let factory_address = app_state.beacon_factory_address;

    let mut deployment_results = Vec::new();

    // Create and deploy perps for multiple beacons
    for i in 0..2 {
        println!("Creating beacon {i} for perp deployment");

        // Create beacon
        let beacon_result =
            create_beacon_via_factory(&app_state, owner_address, factory_address).await;
        if let Ok(beacon_address) = beacon_result {
            println!("Created beacon {i} at: {beacon_address}");

            // Deploy perp for beacon
            let deploy_result = deploy_perp_for_beacon(&app_state, beacon_address).await;
            deployment_results.push((i, beacon_address, deploy_result));
        } else {
            println!("Failed to create beacon {i}: {beacon_result:?}");
        }
    }

    // Analyze results
    let mut successful_deployments = 0;
    for (i, beacon_address, result) in deployment_results {
        match result {
            Ok(response) => {
                println!(
                    "Perp deployment {} succeeded for beacon {}: {}",
                    i, beacon_address, response.transaction_hash
                );
                successful_deployments += 1;
            }
            Err(e) => {
                println!("Perp deployment {i} failed for beacon {beacon_address}: {e}");
            }
        }
    }

    println!("Successful perp deployments: {successful_deployments}");
}

/// Test liquidity deposit functionality
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_deposit_liquidity_for_perp_integration() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Create and deploy a perp first
    let owner_address = app_state.wallet_address;
    let factory_address = app_state.beacon_factory_address;

    let beacon_result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;
    assert!(beacon_result.is_ok(), "Beacon creation should succeed");

    let _beacon_address = beacon_result.unwrap();

    // Use a placeholder perp id for integration flow
    let deposit_request = DepositLiquidityForPerpRequest {
        perp_id: format!("0x{:064}", 1u64),
        margin_amount_usdc: "1000000".to_string(),
    };

    let deposit_result = deposit_liquidity_for_perp(&app_state, deposit_request).await;

    match deposit_result {
        Ok(response) => {
            println!("Liquidity deposit succeeded:");
            println!("  Position ID: {}", response.maker_position_id);
            println!("  Approval hash: {}", response.approval_transaction_hash);
            println!("  Deposit hash: {}", response.deposit_transaction_hash);
            assert!(
                response.approval_transaction_hash.starts_with("0x"),
                "Should have valid approval hash"
            );
            assert!(
                response.deposit_transaction_hash.starts_with("0x"),
                "Should have valid deposit hash"
            );
        }
        Err(e) => {
            println!("Liquidity deposit failed (may be expected): {e}");
            // Should not fail on id parsing
            assert!(
                !e.contains("Invalid perp ID"),
                "Should not be validation error: {e}"
            );
        }
    }
}

/// Test liquidity deposit with invalid perp id
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_deposit_liquidity_invalid_perp() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let deposit_request = DepositLiquidityForPerpRequest {
        perp_id: "invalid_address".to_string(),
        margin_amount_usdc: "1000000".to_string(),
    };

    let result = deposit_liquidity_for_perp(&app_state, deposit_request).await;

    assert!(result.is_err(), "Should fail with invalid perp ID");
    assert!(result.unwrap_err().contains("Invalid perp ID"));
}

/// Test liquidity deposit with zero amounts
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_deposit_liquidity_zero_amount() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let deposit_request = DepositLiquidityForPerpRequest {
        perp_id: format!("0x{:064}", 2u64),
        margin_amount_usdc: "0".to_string(),
    };

    let result = deposit_liquidity_for_perp(&app_state, deposit_request).await;

    // Zero amount expected to be rejected in validation
    assert!(result.is_err());
}

/// Test liquidity deposit with various price ranges (placeholder)
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_deposit_liquidity_price_ranges() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let _pool_address = format!("{:?}", app_state.beacon_factory_address);

    let price_test_cases = vec![
        // (min_price, max_price, description)
        (
            U256::from(1u64 << 95),
            U256::from(1u64 << 97),
            "Normal range",
        ),
        (
            U256::from(1u64 << 96),
            U256::from(1u64 << 96),
            "Equal prices",
        ),
        (U256::from(0), U256::from(u64::MAX), "Maximum range"),
        (U256::from(1), U256::from(2), "Minimal range"),
    ];

    for (i, (_min_price, _max_price, description)) in price_test_cases.into_iter().enumerate() {
        println!("Testing price range {i}: {description}");

        let deposit_request = DepositLiquidityForPerpRequest {
            perp_id: format!("0x{:064}", 100 + i as u64),
            margin_amount_usdc: "1000000".to_string(),
        };

        let result = deposit_liquidity_for_perp(&app_state, deposit_request).await;

        match result {
            Ok(response) => println!(
                "Price range {} succeeded: {}",
                i, response.deposit_transaction_hash
            ),
            Err(e) => {
                println!("Price range {i} failed: {e}");
                // Should not be validation errors
                assert!(!e.contains("Invalid pool address"));
            }
        }
    }
}

/// Test concurrent perp operations
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_concurrent_perp_operations() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let owner_address = app_state.wallet_address;
    let factory_address = app_state.beacon_factory_address;

    // Create a beacon first
    let beacon_result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;
    if let Ok(beacon_address) = beacon_result {
        let mut handles = Vec::new();

        // Start multiple perp deployments for the same beacon
        for i in 0..2 {
            let app_state_clone = app_state.clone();
            let handle = tokio::spawn(async move {
                println!("Starting concurrent perp deployment {i}");
                let result = deploy_perp_for_beacon(&app_state_clone, beacon_address).await;
                (i, result)
            });
            handles.push(handle);
        }

        // Wait for results
        let mut success_count = 0;
        for handle in handles {
            let (i, result) = handle.await.unwrap();
            match result {
                Ok(response) => {
                    println!(
                        "Concurrent perp deployment {} succeeded: {}",
                        i, response.transaction_hash
                    );
                    success_count += 1;
                }
                Err(e) => println!("Concurrent perp deployment {i} failed: {e}"),
            }
        }

        println!("Concurrent perp deployments: {success_count} successes");
    } else {
        println!("Failed to create beacon for concurrent test: {beacon_result:?}");
    }
}

/// Test perp deployment error handling
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_perp_deployment_error_handling() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    // Test with various problematic addresses
    let test_addresses = vec![
        (Address::ZERO, "Zero address"),
        (
            Address::from_str("0x0000000000000000000000000000000000000001").unwrap(),
            "Address 1",
        ),
        (
            Address::from_str("0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF").unwrap(),
            "Max address",
        ),
    ];

    for (address, description) in test_addresses {
        println!("Testing perp deployment with {description}: {address}");

        let result = deploy_perp_for_beacon(&app_state, address).await;

        match result {
            Ok(response) => println!(
                "{} deployment succeeded: {}",
                description, response.transaction_hash
            ),
            Err(e) => {
                println!("{description} deployment failed: {e}");
                // Log error for analysis
            }
        }
    }
}

/// Test perp operations with extreme values
#[tokio::test]
#[ignore] // Temporarily disabled - hangs due to real network calls
#[serial]
async fn test_perp_operations_extreme_values() {
    let (app_state, _manager) = crate::test_utils::create_isolated_test_app_state().await;

    let _pool_address = format!("{:?}", app_state.beacon_factory_address);

    // Test with extreme USDC amounts (string margins)
    let amount_test_cases = vec![
        ("1".to_string(), "Minimum amount"),
        ("1000000".to_string(), "1 Million (scaled)"),
        (u64::MAX.to_string(), "Maximum u64 as string"),
    ];

    for (amount, description) in amount_test_cases {
        println!("Testing liquidity deposit with {description}: {amount}");

        let deposit_request = DepositLiquidityForPerpRequest {
            perp_id: format!("0x{:064}", 3u64),
            margin_amount_usdc: amount,
        };

        let result = deposit_liquidity_for_perp(&app_state, deposit_request).await;

        match result {
            Ok(response) => println!(
                "{} deposit succeeded: {}",
                description, response.deposit_transaction_hash
            ),
            Err(e) => {
                println!("{description} deposit failed: {e}");
                // Should not be validation errors for id formatting
                assert!(!e.contains("Invalid pool address"));
            }
        }
    }
}
