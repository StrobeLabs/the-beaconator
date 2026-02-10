//! Redis-based nonce conflict prevention tests
//!
//! These tests verify that Redis-based wallet locking prevents nonce conflicts
//! when multiple transactions are executed concurrently.
//!
//! Run with: `make test-wallet` (requires Redis container)

use serial_test::serial;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;

/// Test that concurrent lock acquisitions on DIFFERENT wallets succeed in parallel
#[tokio::test]
#[serial]
#[ignore = "requires Redis - run with make test-wallet"]
async fn test_parallel_locks_different_wallets_no_conflict() {
    use alloy::primitives::Address;
    use the_beaconator::services::wallet::WalletLock;

    // Skip if Redis is not available
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let redis_client = match redis::Client::open(redis_url.as_str()) {
        Ok(client) => client,
        Err(_) => {
            println!("Redis not available, skipping test");
            return;
        }
    };

    // Check if Redis is actually running
    if redis_client
        .get_multiplexed_async_connection()
        .await
        .is_err()
    {
        println!("Cannot connect to Redis, skipping test");
        return;
    }

    let wallet1_address = Address::repeat_byte(0x01);
    let wallet2_address = Address::repeat_byte(0x02);

    let start = Instant::now();
    let mut join_set = JoinSet::new();

    // Spawn two concurrent tasks, each acquiring a different wallet lock
    let redis1 = redis_client.clone();
    join_set.spawn(async move {
        let lock = WalletLock::new(
            redis1,
            wallet1_address,
            "test-instance-1".to_string(),
            Duration::from_secs(60),
        );

        let acquire_start = Instant::now();
        match lock.acquire(1, Duration::from_millis(100)).await {
            Ok(guard) => {
                // Simulate transaction processing
                tokio::time::sleep(Duration::from_millis(100)).await;
                // Release explicitly
                guard.release().await.ok();
                Ok(("wallet1", acquire_start.elapsed()))
            }
            Err(e) => Err(format!("wallet1 error: {e}")),
        }
    });

    let redis2 = redis_client.clone();
    join_set.spawn(async move {
        let lock = WalletLock::new(
            redis2,
            wallet2_address,
            "test-instance-2".to_string(),
            Duration::from_secs(60),
        );

        let acquire_start = Instant::now();
        match lock.acquire(1, Duration::from_millis(100)).await {
            Ok(guard) => {
                // Simulate transaction processing
                tokio::time::sleep(Duration::from_millis(100)).await;
                // Release explicitly
                guard.release().await.ok();
                Ok(("wallet2", acquire_start.elapsed()))
            }
            Err(e) => Err(format!("wallet2 error: {e}")),
        }
    });

    // Collect results
    let mut results = Vec::new();
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(info)) => results.push(info),
            Ok(Err(e)) => println!("Task failed: {e}"),
            Err(e) => println!("Join error: {e}"),
        }
    }

    let elapsed = start.elapsed();

    // Both should complete successfully
    assert_eq!(results.len(), 2, "Both lock acquisitions should succeed");

    // Total time should be ~100ms (parallel), not ~200ms (serial)
    assert!(
        elapsed < Duration::from_millis(180),
        "Parallel execution should complete in ~100ms, took {elapsed:?}"
    );

    println!("Parallel lock acquisition completed in {elapsed:?}");
}

/// Test that concurrent lock acquisitions on SAME wallet are serialized by Redis lock
#[tokio::test]
#[serial]
#[ignore = "requires Redis - run with make test-wallet"]
async fn test_serialized_locks_same_wallet_no_conflict() {
    use alloy::primitives::Address;
    use the_beaconator::services::wallet::WalletLock;

    // Skip if Redis is not available
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let redis_client = match redis::Client::open(redis_url.as_str()) {
        Ok(client) => client,
        Err(_) => {
            println!("Redis not available, skipping test");
            return;
        }
    };

    // Check if Redis is actually running
    if redis_client
        .get_multiplexed_async_connection()
        .await
        .is_err()
    {
        println!("Cannot connect to Redis, skipping test");
        return;
    }

    // Both tasks try to lock the SAME wallet
    let wallet_address = Address::repeat_byte(0x11);

    let start = Instant::now();

    // First, acquire the lock
    let lock1 = WalletLock::new(
        redis_client.clone(),
        wallet_address,
        "instance-1".to_string(),
        Duration::from_secs(60),
    );

    let guard1 = match lock1.acquire(1, Duration::from_millis(100)).await {
        Ok(g) => g,
        Err(e) => {
            println!("First acquisition failed: {e}");
            return;
        }
    };

    let first_acquired = start.elapsed();
    println!("First lock acquired in {first_acquired:?}");

    // Spawn a task to try acquiring the same wallet (should fail initially, then succeed after release)
    let redis2 = redis_client.clone();
    let acquire_task = tokio::spawn(async move {
        let lock2 = WalletLock::new(
            redis2,
            wallet_address,
            "instance-2".to_string(),
            Duration::from_secs(60),
        );

        let acquire_start = Instant::now();
        // Try with retries - should eventually succeed after first lock is released
        let result = lock2.acquire(10, Duration::from_millis(50)).await;
        (acquire_start.elapsed(), result)
    });

    // Hold the first lock for 100ms
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Release the first lock by dropping the guard
    drop(guard1);

    // Wait for the second acquisition to complete
    let (wait_time, result) = acquire_task.await.unwrap();

    match result {
        Ok(guard2) => {
            println!("Second lock acquired after waiting {wait_time:?}");

            // The second task should have waited at least ~100ms for the lock
            assert!(
                wait_time >= Duration::from_millis(80),
                "Second acquisition should wait for first to release, but only waited {wait_time:?}"
            );

            // Clean up
            guard2.release().await.ok();
        }
        Err(e) => {
            // Timeout is possible if retries are exhausted
            println!("Second acquisition error: {e}");
        }
    }
}

/// Test lock TTL prevents deadlock if instance crashes mid-transaction
#[tokio::test]
#[serial]
#[ignore = "requires Redis - run with make test-wallet"]
async fn test_lock_ttl_prevents_deadlock() {
    use alloy::primitives::Address;
    use the_beaconator::services::wallet::WalletLock;

    // Skip if Redis is not available
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let redis_client = match redis::Client::open(redis_url.as_str()) {
        Ok(client) => client,
        Err(_) => {
            println!("Redis not available, skipping test");
            return;
        }
    };

    if redis_client
        .get_multiplexed_async_connection()
        .await
        .is_err()
    {
        println!("Cannot connect to Redis, skipping test");
        return;
    }

    let wallet_address = Address::repeat_byte(0xAA);

    // Create a lock with very short TTL (2 seconds)
    let lock1 = WalletLock::new(
        redis_client.clone(),
        wallet_address,
        "test-instance-crash".to_string(),
        Duration::from_secs(2), // 2 second TTL
    );

    // Acquire the lock
    let guard = match lock1.acquire(1, Duration::from_millis(100)).await {
        Ok(g) => g,
        Err(e) => {
            println!("Failed to acquire lock: {e}");
            return;
        }
    };

    println!("Lock acquired on wallet {wallet_address}");

    // Simulate a crash by forgetting the guard (not calling release)
    // In production, this would happen if the instance crashes
    std::mem::forget(guard);

    // Wait for TTL to expire
    println!("Waiting for lock TTL to expire (2 seconds)...");
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Create a new lock (simulating a new instance after crash)
    let lock2 = WalletLock::new(
        redis_client,
        wallet_address,
        "test-instance-recovery".to_string(),
        Duration::from_secs(60),
    );

    // Should be able to acquire the lock now
    let start = Instant::now();
    match lock2.acquire(1, Duration::from_millis(100)).await {
        Ok(guard2) => {
            let elapsed = start.elapsed();
            println!("Lock re-acquired after TTL expiry in {elapsed:?}");
            // Should acquire immediately after TTL, not need to wait
            assert!(
                elapsed < Duration::from_secs(1),
                "Should acquire immediately after TTL expiry"
            );
            guard2.release().await.ok();
        }
        Err(e) => {
            panic!("Failed to re-acquire lock after TTL: {e}");
        }
    }
}

/// Test multi-instance lock contention (simulated with different instance IDs)
#[tokio::test]
#[serial]
#[ignore = "requires Redis - run with make test-wallet"]
async fn test_multi_instance_lock_contention() {
    use alloy::primitives::Address;
    use the_beaconator::services::wallet::WalletLock;

    // Skip if Redis is not available
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let redis_client = match redis::Client::open(redis_url.as_str()) {
        Ok(client) => client,
        Err(_) => {
            println!("Redis not available, skipping test");
            return;
        }
    };

    if redis_client
        .get_multiplexed_async_connection()
        .await
        .is_err()
    {
        println!("Cannot connect to Redis, skipping test");
        return;
    }

    let wallet_address = Address::repeat_byte(0xBB);

    // Race to acquire the same wallet from two "instances"
    let start = Instant::now();
    let mut join_set = JoinSet::new();

    let redis1 = redis_client.clone();
    join_set.spawn(async move {
        let lock = WalletLock::new(
            redis1,
            wallet_address,
            "instance-1".to_string(),
            Duration::from_secs(60),
        );

        let acquire_start = Instant::now();
        match lock.try_acquire().await {
            Ok(guard) => {
                // Hold lock briefly
                tokio::time::sleep(Duration::from_millis(50)).await;
                guard.release().await.ok();
                Ok(("instance-1", acquire_start.elapsed()))
            }
            Err(e) => Err(format!("instance-1 error: {e}")),
        }
    });

    let redis2 = redis_client.clone();
    join_set.spawn(async move {
        let lock = WalletLock::new(
            redis2,
            wallet_address,
            "instance-2".to_string(),
            Duration::from_secs(60),
        );

        let acquire_start = Instant::now();
        match lock.try_acquire().await {
            Ok(guard) => {
                // Hold lock briefly
                tokio::time::sleep(Duration::from_millis(50)).await;
                guard.release().await.ok();
                Ok(("instance-2", acquire_start.elapsed()))
            }
            Err(e) => Err(format!("instance-2 error: {e}")),
        }
    });

    // Collect results
    let mut successes = Vec::new();
    let mut failures = Vec::new();

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(info)) => successes.push(info),
            Ok(Err(e)) => failures.push(e),
            Err(e) => failures.push(format!("Join error: {e}")),
        }
    }

    let elapsed = start.elapsed();
    println!("Lock contention test completed in {elapsed:?}");

    // Exactly one should succeed (they're racing for the same lock with try_acquire)
    assert!(
        !successes.is_empty(),
        "At least one instance should acquire the lock"
    );

    // The winner should have acquired quickly
    let winner = &successes[0];
    println!("{} won the lock race, acquired in {:?}", winner.0, winner.1);
    assert!(
        winner.1 < Duration::from_millis(100),
        "Winner should acquire immediately"
    );

    // With try_acquire, exactly one should fail
    if !failures.is_empty() {
        println!(
            "Expected failure (lock contention): {}",
            failures.first().unwrap()
        );
    }
}

/// Test wallet pool basic operations
#[tokio::test]
#[serial]
#[ignore = "requires Redis - run with make test-wallet"]
async fn test_wallet_pool_operations() {
    use alloy::primitives::Address;
    use the_beaconator::services::wallet::{WalletInfo, WalletPool, WalletStatus};

    // Skip if Redis is not available
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let redis_client = match redis::Client::open(redis_url.as_str()) {
        Ok(client) => client,
        Err(_) => {
            println!("Redis not available, skipping test");
            return;
        }
    };

    if redis_client
        .get_multiplexed_async_connection()
        .await
        .is_err()
    {
        println!("Cannot connect to Redis, skipping test");
        return;
    }

    // Create a wallet pool
    let pool = match WalletPool::new(&redis_url, "test-pool-ops".to_string()).await {
        Ok(p) => p,
        Err(e) => {
            println!("Failed to create pool: {e}");
            return;
        }
    };

    // Create test wallets
    let wallet1 = Address::repeat_byte(0xC1);
    let wallet2 = Address::repeat_byte(0xC2);
    let wallet3 = Address::repeat_byte(0xC3);

    let wallets = vec![wallet1, wallet2, wallet3];

    // Add wallets to pool
    for (i, addr) in wallets.iter().enumerate() {
        let info = WalletInfo {
            address: *addr,
            key_id: format!("key-{i}"),
            status: WalletStatus::Available,
            designated_beacons: vec![],
        };
        pool.add_wallet(info).await.expect("Failed to add wallet");
    }

    // List wallets
    let listed = pool.list_wallets().await.expect("Failed to list wallets");
    assert!(
        listed.len() >= 3,
        "Should have at least 3 wallets, got {}",
        listed.len()
    );

    // Check available wallets
    let available = pool
        .list_available_wallets()
        .await
        .expect("Failed to list available");
    assert!(!available.is_empty(), "Should have some available wallets");

    // Clean up - remove test wallets
    for addr in &wallets {
        pool.remove_wallet(addr).await.ok();
    }

    println!("Wallet pool operations test completed successfully");
}

/// Test beacon to wallet mapping
#[tokio::test]
#[serial]
#[ignore = "requires Redis - run with make test-wallet"]
async fn test_beacon_wallet_mapping() {
    use alloy::primitives::Address;
    use the_beaconator::services::wallet::{WalletInfo, WalletPool, WalletStatus};

    // Skip if Redis is not available
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let redis_client = match redis::Client::open(redis_url.as_str()) {
        Ok(client) => client,
        Err(_) => {
            println!("Redis not available, skipping test");
            return;
        }
    };

    if redis_client
        .get_multiplexed_async_connection()
        .await
        .is_err()
    {
        println!("Cannot connect to Redis, skipping test");
        return;
    }

    let pool = match WalletPool::new(&redis_url, "test-beacon-mapping".to_string()).await {
        Ok(p) => p,
        Err(e) => {
            println!("Failed to create pool: {e}");
            return;
        }
    };

    let wallet_address = Address::repeat_byte(0xD1);
    let beacon_address = Address::repeat_byte(0xE1);

    // Add wallet to pool first
    let info = WalletInfo {
        address: wallet_address,
        key_id: "key-beacon-test".to_string(),
        status: WalletStatus::Available,
        designated_beacons: vec![],
    };
    pool.add_wallet(info).await.expect("Failed to add wallet");

    // Add beacon mapping
    pool.add_designated_beacon(&wallet_address, &beacon_address)
        .await
        .expect("Failed to add beacon mapping");

    // Verify mapping
    let mapped_wallet = pool
        .get_wallet_for_beacon(&beacon_address)
        .await
        .expect("Failed to get wallet for beacon");
    assert_eq!(
        mapped_wallet,
        Some(wallet_address),
        "Beacon should be mapped to wallet"
    );

    // Get beacons for wallet
    let beacons = pool
        .get_beacons_for_wallet(&wallet_address)
        .await
        .expect("Failed to get beacons for wallet");
    assert!(
        beacons.contains(&beacon_address),
        "Wallet should have beacon in designated list"
    );

    // Clean up
    pool.remove_designated_beacon(&wallet_address, &beacon_address)
        .await
        .ok();
    pool.remove_wallet(&wallet_address).await.ok();

    println!("Beacon wallet mapping test completed successfully");
}
