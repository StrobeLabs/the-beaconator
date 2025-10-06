use alloy::primitives::{Address, B256, Bytes};
use alloy::providers::Provider;
use std::{str::FromStr, time::Duration};
use tokio::time::timeout;
use tracing;

use crate::models::{AppState, UpdateBeaconRequest};
use crate::routes::{
    IBeacon, IBeaconFactory, IBeaconRegistry, execute_transaction_serialized,
    get_fresh_nonce_from_alternate, is_nonce_error,
};
use crate::services::transaction::events::parse_beacon_created_event;

/// Create a beacon via the factory contract
///
/// This function handles:
/// - Transaction execution with RPC fallback
/// - Transaction confirmation with progressive timeouts
/// - Event parsing to extract beacon address
pub async fn create_beacon_via_factory(
    state: &AppState,
    owner_address: Address,
    factory_address: Address,
) -> Result<Address, String> {
    tracing::info!(
        "Creating beacon via factory {} for owner {}",
        factory_address,
        owner_address
    );

    // Create contract instance using the sol! generated interface
    let contract = IBeaconFactory::new(factory_address, &*state.provider);

    // Send the beacon creation transaction with RPC fallback (serialized)
    let pending_tx = execute_transaction_serialized(async {
        // Try primary RPC first
        tracing::info!("Creating beacon with primary RPC");
        let result = contract.createBeacon(owner_address).send().await;

        match result {
            Ok(pending) => Ok(pending),
            Err(e) => {
                let error_msg = format!("Failed to send createBeacon transaction: {e}");
                tracing::error!("{}", error_msg);

                // Check if nonce error and sync if needed
                if is_nonce_error(&error_msg) {
                    tracing::warn!("Nonce error detected, waiting before fallback");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }

                // Try alternate RPC if available
                if let Some(alternate_provider) = &state.alternate_provider {
                    tracing::info!("Trying beacon creation with alternate RPC");

                    // Get fresh nonce from alternate RPC to avoid nonce conflicts
                    if let Err(nonce_error) = get_fresh_nonce_from_alternate(state).await {
                        tracing::warn!("Could not sync nonce with alternate RPC: {}", nonce_error);
                    }

                    let alt_contract = IBeaconFactory::new(factory_address, &**alternate_provider);

                    match alt_contract.createBeacon(owner_address).send().await {
                        Ok(pending) => {
                            tracing::info!("Beacon creation succeeded with alternate RPC");
                            Ok(pending)
                        }
                        Err(alt_e) => {
                            let combined_error = format!(
                                "Beacon creation failed on both RPCs. Primary: {e}. Alternate: {alt_e}"
                            );
                            tracing::error!("{}", combined_error);
                            sentry::capture_message(&combined_error, sentry::Level::Error);
                            Err(combined_error)
                        }
                    }
                } else {
                    tracing::error!("No alternate RPC configured, cannot fallback");
                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    Err(error_msg)
                }
            }
        }
    })
    .await?;

    tracing::info!("Transaction sent, waiting for receipt...");

    // Get the transaction hash before calling get_receipt() (which takes ownership)
    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("Transaction hash: {:?}", tx_hash);

    // Use get_receipt() with timeout and fallback to on-chain check
    let receipt = match timeout(Duration::from_secs(60), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => {
            tracing::info!("Transaction confirmed via get_receipt()");
            receipt
        }
        Ok(Err(e)) => {
            tracing::warn!("get_receipt() failed: {}", e);
            tracing::info!("Falling back to on-chain transaction check...");

            tracing::info!("Checking transaction {} on-chain...", tx_hash);

            // Try to get the receipt directly from the provider with timeout
            match timeout(
                Duration::from_secs(30),
                state.provider.get_transaction_receipt(tx_hash),
            )
            .await
            {
                Ok(Ok(Some(receipt))) => {
                    tracing::info!("Transaction found on-chain via direct receipt lookup");
                    receipt
                }
                Ok(Ok(None)) => {
                    let error_msg =
                        format!("Transaction {tx_hash} not found on-chain after timeout");
                    tracing::error!("{}", error_msg);
                    tracing::error!("This could indicate:");
                    tracing::error!("  - Transaction was dropped/replaced");
                    tracing::error!("  - Network issues prevented confirmation");
                    tracing::error!("  - Transaction is still pending");
                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    return Err(error_msg);
                }
                Ok(Err(e)) => {
                    let error_msg = format!("Failed to check transaction {tx_hash} on-chain: {e}");
                    tracing::error!("{}", error_msg);
                    tracing::error!("Original get_receipt() error: {}", e);
                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    return Err(error_msg);
                }
                Err(_) => {
                    let error_msg = format!("Timeout checking transaction {tx_hash} on-chain");
                    tracing::error!("{}", error_msg);
                    tracing::error!("Network may be slow or unresponsive");
                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    return Err(error_msg);
                }
            }
        }
        Err(_) => {
            tracing::warn!(
                "Initial get_receipt() timed out for beacon transaction, trying extended fallback..."
            );
            tracing::info!(
                "Checking beacon transaction {} on-chain with progressive timeouts...",
                tx_hash
            );

            // Extended fallback: retry with progressive timeouts (15s, 30s, 60s) for Base network
            let mut retry_count = 0;
            let max_retries = 3;
            let timeout_seconds = [15u64, 30u64, 60u64]; // Progressive timeout pattern

            loop {
                retry_count += 1;
                let current_timeout = timeout_seconds[retry_count - 1];
                tracing::info!(
                    "Beacon transaction receipt attempt {}/{} ({}s timeout)",
                    retry_count,
                    max_retries,
                    current_timeout
                );

                match timeout(
                    Duration::from_secs(current_timeout),
                    is_transaction_confirmed(state, tx_hash),
                )
                .await
                {
                    Ok(Ok(Some(receipt))) => {
                        tracing::info!(
                            "Beacon transaction found on-chain via extended fallback (attempt {})",
                            retry_count
                        );
                        break receipt;
                    }
                    Ok(Ok(None)) => {
                        if retry_count >= max_retries {
                            let error_msg = format!(
                                "Beacon transaction {tx_hash} not found on-chain after {max_retries} attempts"
                            );
                            tracing::error!("{}", error_msg);
                            tracing::error!("This could indicate:");
                            tracing::error!("  - Beacon transaction was dropped/replaced");
                            tracing::error!("  - Network issues prevented confirmation");
                            tracing::error!("  - Transaction is still pending (check gas price)");
                            tracing::error!("  - Base network congestion causing delays");
                            return Err(error_msg);
                        }
                        tracing::warn!(
                            "Beacon transaction not found on attempt {}, retrying...",
                            retry_count
                        );
                        tokio::time::sleep(Duration::from_secs(3)).await; // Brief pause between retries
                    }
                    Ok(Err(e)) => {
                        let error_msg =
                            format!("Failed to check beacon transaction {tx_hash} on-chain: {e}");
                        tracing::error!("{}", error_msg);
                        return Err(error_msg);
                    }
                    Err(_) => {
                        if retry_count >= max_retries {
                            let error_msg = format!(
                                "Final timeout waiting for beacon transaction receipt {tx_hash} after {max_retries} attempts"
                            );
                            tracing::error!("{}", error_msg);
                            tracing::error!(
                                "All fallback methods exhausted for beacon transaction"
                            );
                            return Err(error_msg);
                        }
                        tracing::warn!("Timeout on attempt {}, retrying...", retry_count);
                        tokio::time::sleep(Duration::from_secs(3)).await; // Brief pause between retries
                    }
                }
            }
        }
    };

    let tx_hash = receipt.transaction_hash;
    tracing::info!("Transaction confirmed with hash: {:?}", tx_hash);

    tracing::info!(
        "Beacon creation confirmed in block {:?}",
        receipt.block_number
    );

    // Validate transaction status before parsing events
    if receipt.status() {
        tracing::info!("Beacon creation transaction succeeded (status: true)");

        // Parse the beacon address from the event logs
        let beacon_address = parse_beacon_created_event(&receipt, factory_address)?;

        tracing::info!("Beacon created at address: {}", beacon_address);
        sentry::capture_message(
            &format!("Beacon created via factory: {beacon_address}"),
            sentry::Level::Info,
        );
        Ok(beacon_address)
    } else {
        let error_msg = format!(
            "Beacon creation transaction {tx_hash} reverted (status: false) in block {:?}",
            receipt.block_number
        );
        tracing::error!("{}", error_msg);
        tracing::error!("Factory: {}, Owner: {}", factory_address, owner_address);
        sentry::capture_message(&error_msg, sentry::Level::Error);
        Err(error_msg)
    }
}

/// Check if a transaction is already confirmed on-chain
pub async fn is_transaction_confirmed(
    state: &AppState,
    tx_hash: B256,
) -> Result<Option<alloy::rpc::types::TransactionReceipt>, String> {
    tracing::info!(
        "Checking if transaction {} is already confirmed on-chain...",
        tx_hash
    );

    match state.provider.get_transaction_receipt(tx_hash).await {
        Ok(Some(receipt)) => {
            tracing::info!(
                "Transaction {} is confirmed in block {}",
                tx_hash,
                receipt.block_number.unwrap_or(0)
            );
            Ok(Some(receipt))
        }
        Ok(None) => {
            tracing::info!(
                "Transaction {} not found on-chain (may be pending or dropped)",
                tx_hash
            );
            Ok(None)
        }
        Err(e) => {
            let error_msg = format!("Failed to check transaction {tx_hash} on-chain: {e}");
            tracing::error!("{}", error_msg);
            Err(error_msg)
        }
    }
}

/// Check if a beacon is already registered with a registry
pub async fn is_beacon_registered(
    state: &AppState,
    beacon_address: Address,
    registry_address: Address,
) -> Result<bool, String> {
    tracing::info!(
        "Checking if beacon {} is already registered...",
        beacon_address
    );

    // Create contract instance and call beacons(address) directly
    let contract = IBeaconRegistry::new(registry_address, &*state.provider);

    match contract.beacons(beacon_address).call().await {
        Ok(is_registered) => {
            if is_registered {
                tracing::info!("Beacon {} is already registered", beacon_address);
            } else {
                tracing::info!("Beacon {} is not registered", beacon_address);
            }
            Ok(is_registered)
        }
        Err(e) => {
            tracing::warn!(
                "Failed to check beacon registration status: {}. Assuming not registered.",
                e
            );
            // If we can't check, assume it's not registered to allow the operation to proceed
            Ok(false)
        }
    }
}

/// Register a beacon with a registry
///
/// This function handles:
/// - Pre-registration validation (check if already registered)
/// - Transaction execution with RPC fallback
/// - Transaction confirmation with progressive timeouts
pub async fn register_beacon_with_registry(
    state: &AppState,
    beacon_address: Address,
    registry_address: Address,
) -> Result<B256, String> {
    tracing::info!(
        "Registering beacon {} with registry {}",
        beacon_address,
        registry_address
    );

    // Pre-registration validation
    tracing::info!("Pre-registration validation:");
    tracing::info!("  - Beacon address: {}", beacon_address);
    tracing::info!("  - Registry address: {}", registry_address);
    tracing::info!("  - Wallet address: {}", state.wallet_address);

    // Check if beacon is already registered
    tracing::info!("Checking if beacon is already registered...");
    let is_registered = is_beacon_registered(state, beacon_address, registry_address).await?;

    if is_registered {
        tracing::info!(
            "Beacon {} is already registered with registry {}, returning success",
            beacon_address,
            registry_address
        );
        // Return a fake transaction hash to indicate success without actual transaction
        // Using zeros is a common pattern to indicate no-op success
        return Ok(B256::ZERO);
    }

    // Validate beacon contract exists and has code
    tracing::info!("Validating beacon contract...");
    match state.provider.get_code_at(beacon_address).await {
        Ok(code) => {
            if code.is_empty() {
                let error_msg = format!("Beacon address {beacon_address} has no deployed code");
                tracing::error!("{}", error_msg);
                return Err(error_msg);
            } else {
                tracing::info!("Beacon contract has {} bytes of code", code.len());
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to check beacon contract: {e}");
            tracing::error!("{}", error_msg);
            return Err(error_msg);
        }
    }

    // Create contract instance using the sol! generated interface
    let contract = IBeaconRegistry::new(registry_address, &*state.provider);

    // Send the registration transaction with RPC fallback (serialized)
    let pending_tx = execute_transaction_serialized(async {
        // Try primary RPC first
        tracing::info!("Registering beacon with primary RPC");
        let result = contract.registerBeacon(beacon_address).send().await;

        match result {
            Ok(pending) => Ok(pending),
            Err(e) => {
                let error_msg = format!("Failed to send registerBeacon transaction: {e}");
                tracing::error!("{}", error_msg);

                // Check if nonce error and sync if needed
                if is_nonce_error(&error_msg) {
                    tracing::warn!("Nonce error detected, waiting before fallback");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }

                // Try alternate RPC if available
                if let Some(alternate_provider) = &state.alternate_provider {
                    tracing::info!("Trying beacon registration with alternate RPC");

                    // Get fresh nonce from alternate RPC to avoid nonce conflicts
                    if let Err(nonce_error) = get_fresh_nonce_from_alternate(state).await {
                        tracing::warn!("Could not sync nonce with alternate RPC: {}", nonce_error);
                    }

                    let alt_contract =
                        IBeaconRegistry::new(registry_address, &**alternate_provider);

                    match alt_contract.registerBeacon(beacon_address).send().await {
                        Ok(pending) => {
                            tracing::info!("Beacon registration succeeded with alternate RPC");
                            Ok(pending)
                        }
                        Err(alt_e) => {
                            let combined_error = format!(
                                "Beacon registration failed on both RPCs. Primary: {e}. Alternate: {alt_e}"
                            );
                            tracing::error!("{}", combined_error);
                            sentry::capture_message(&combined_error, sentry::Level::Error);
                            Err(combined_error)
                        }
                    }
                } else {
                    tracing::error!("No alternate RPC configured, cannot fallback");
                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    Err(error_msg)
                }
            }
        }
    })
    .await?;

    tracing::info!("Registration transaction sent, waiting for receipt...");

    // Get the transaction hash before calling get_receipt() (which takes ownership)
    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("Registration transaction hash: {:?}", tx_hash);

    // Use get_receipt() with timeout and fallback to on-chain check
    let receipt = match timeout(Duration::from_secs(60), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => {
            tracing::info!("Registration confirmed via get_receipt()");
            receipt
        }
        Ok(Err(e)) => {
            tracing::warn!("get_receipt() failed for registration: {}", e);
            tracing::info!("Falling back to on-chain registration check...");

            tracing::info!("Checking registration transaction {} on-chain...", tx_hash);

            // Try to get the receipt directly from the provider with timeout
            match timeout(
                Duration::from_secs(30),
                state.provider.get_transaction_receipt(tx_hash),
            )
            .await
            {
                Ok(Ok(Some(receipt))) => {
                    tracing::info!("Registration found on-chain via direct receipt lookup");
                    receipt
                }
                Ok(Ok(None)) => {
                    let error_msg = format!(
                        "Registration transaction {tx_hash} not found on-chain after timeout"
                    );
                    tracing::error!("{}", error_msg);
                    tracing::error!("This could indicate:");
                    tracing::error!("  - Registration transaction was dropped/replaced");
                    tracing::error!("  - Network issues prevented confirmation");
                    tracing::error!("  - Registration is still pending");
                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    return Err(error_msg);
                }
                Ok(Err(e)) => {
                    let error_msg =
                        format!("Failed to check registration transaction {tx_hash} on-chain: {e}");
                    tracing::error!("{}", error_msg);
                    tracing::error!("Original get_receipt() error: {}", e);
                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    return Err(error_msg);
                }
                Err(_) => {
                    let error_msg =
                        format!("Timeout checking registration transaction {tx_hash} on-chain");
                    tracing::error!("{}", error_msg);
                    tracing::error!("Network may be slow or unresponsive");
                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    return Err(error_msg);
                }
            }
        }
        Err(_) => {
            tracing::warn!(
                "Initial get_receipt() timed out for registration transaction, trying extended fallback..."
            );
            tracing::info!(
                "Checking registration transaction {} on-chain with progressive timeouts...",
                tx_hash
            );

            // Extended fallback: retry with progressive timeouts (15s, 30s, 60s) for Base network
            let mut retry_count = 0;
            let max_retries = 3;
            let timeout_seconds = [15u64, 30u64, 60u64]; // Progressive timeout pattern

            loop {
                retry_count += 1;
                let current_timeout = timeout_seconds[retry_count - 1];
                tracing::info!(
                    "Registration transaction receipt attempt {}/{} ({}s timeout)",
                    retry_count,
                    max_retries,
                    current_timeout
                );

                match timeout(
                    Duration::from_secs(current_timeout),
                    is_transaction_confirmed(state, tx_hash),
                )
                .await
                {
                    Ok(Ok(Some(receipt))) => {
                        tracing::info!(
                            "Registration transaction found on-chain via extended fallback (attempt {})",
                            retry_count
                        );
                        break receipt;
                    }
                    Ok(Ok(None)) => {
                        if retry_count >= max_retries {
                            let error_msg = format!(
                                "Registration transaction {tx_hash} not found on-chain after {max_retries} attempts"
                            );
                            tracing::error!("{}", error_msg);
                            tracing::error!("This could indicate:");
                            tracing::error!("  - Registration transaction was dropped/replaced");
                            tracing::error!("  - Network issues prevented confirmation");
                            tracing::error!("  - Transaction is still pending (check gas price)");
                            tracing::error!("  - Base network congestion causing delays");
                            return Err(error_msg);
                        }
                        tracing::warn!(
                            "Registration transaction not found on attempt {}, retrying...",
                            retry_count
                        );
                        tokio::time::sleep(Duration::from_secs(3)).await; // Brief pause between retries
                    }
                    Ok(Err(e)) => {
                        let error_msg = format!(
                            "Failed to check registration transaction {tx_hash} on-chain: {e}"
                        );
                        tracing::error!("{}", error_msg);
                        return Err(error_msg);
                    }
                    Err(_) => {
                        if retry_count >= max_retries {
                            let error_msg = format!(
                                "Final timeout waiting for registration transaction receipt {tx_hash} after {max_retries} attempts"
                            );
                            tracing::error!("{}", error_msg);
                            tracing::error!(
                                "All fallback methods exhausted for registration transaction"
                            );
                            return Err(error_msg);
                        }
                        tracing::warn!("Timeout on attempt {}, retrying...", retry_count);
                        tokio::time::sleep(Duration::from_secs(3)).await; // Brief pause between retries
                    }
                }
            }
        }
    };

    let tx_hash = receipt.transaction_hash;
    tracing::info!(
        "Registration transaction confirmed with hash: {:?}",
        tx_hash
    );
    tracing::info!("Registration confirmed in block {:?}", receipt.block_number);

    // Check transaction status - only success if true
    if receipt.status() {
        tracing::info!("Registration transaction succeeded (status: true)");
        sentry::capture_message(
            &format!("Beacon {beacon_address} registered with registry {registry_address}"),
            sentry::Level::Info,
        );
        Ok(tx_hash)
    } else {
        let error_msg = format!("Registration transaction {tx_hash} reverted (status: false)");
        tracing::error!("{}", error_msg);
        tracing::error!("Beacon: {}, Registry: {}", beacon_address, registry_address);
        sentry::capture_message(&error_msg, sentry::Level::Error);
        Err(error_msg)
    }
}

/// Updates a beacon with new data using a proof.
///
/// This function handles:
/// - Address validation
/// - Transaction execution with RPC fallback
/// - Transaction confirmation with progressive timeouts
pub async fn update_beacon(state: &AppState, request: UpdateBeaconRequest) -> Result<B256, String> {
    // Parse the beacon address
    let beacon_address = match Address::from_str(&request.beacon_address) {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("Invalid beacon address: {}", e);
            return Err("Invalid beacon address".to_string());
        }
    };

    tracing::info!("Updating beacon {} with proof data", beacon_address);

    // Prepare the proof and public signals
    let proof_bytes = Bytes::from(request.proof.clone());
    let public_signals_bytes = Bytes::from(vec![0u8; 32]); // Placeholder for now

    // Create contract instance using the sol! generated interface
    let contract = IBeacon::new(beacon_address, &*state.provider);

    // Send the update transaction with RPC fallback (serialized)
    let pending_tx = execute_transaction_serialized(async {
        // Try primary RPC first
        tracing::info!("Updating beacon with primary RPC");
        let result = contract
            .updateData(proof_bytes.clone(), public_signals_bytes.clone())
            .send()
            .await;

        match result {
            Ok(pending) => Ok(pending),
            Err(e) => {
                let error_msg = format!("Failed to send updateData transaction: {e}");
                tracing::error!("{}", error_msg);

                // Check if nonce error and sync if needed
                if is_nonce_error(&error_msg) {
                    tracing::warn!("Nonce error detected, waiting before fallback");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }

                Err(error_msg)
            }
        }
    })
    .await?;

    tracing::info!("Transaction sent, waiting for receipt...");

    // Get the transaction hash before calling get_receipt() (which takes ownership)
    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("Transaction hash: {:?}", tx_hash);

    // Use get_receipt() with timeout and fallback to on-chain check
    let receipt = match timeout(Duration::from_secs(60), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => {
            tracing::info!("Transaction confirmed via get_receipt()");
            receipt
        }
        Ok(Err(e)) => {
            tracing::warn!("get_receipt() failed: {}", e);
            tracing::info!("Falling back to on-chain transaction check...");

            tracing::info!("Checking transaction {} on-chain...", tx_hash);

            // Try to get the receipt directly from the provider with timeout
            match timeout(
                Duration::from_secs(30),
                state.provider.get_transaction_receipt(tx_hash),
            )
            .await
            {
                Ok(Ok(Some(receipt))) => {
                    tracing::info!("Transaction found on-chain via direct receipt lookup");
                    receipt
                }
                Ok(Ok(None)) => {
                    let error_msg =
                        format!("Transaction {tx_hash} not found on-chain after timeout");
                    tracing::error!("{}", error_msg);
                    return Err(error_msg);
                }
                Ok(Err(e)) => {
                    let error_msg = format!("Failed to check transaction {tx_hash} on-chain: {e}");
                    tracing::error!("{}", error_msg);
                    return Err(error_msg);
                }
                Err(_) => {
                    let error_msg = format!("Timeout checking transaction {tx_hash} on-chain");
                    tracing::error!("{}", error_msg);
                    return Err(error_msg);
                }
            }
        }
        Err(_) => {
            let error_msg = format!("Timeout waiting for transaction {tx_hash} receipt");
            tracing::error!("{}", error_msg);
            return Err(error_msg);
        }
    };

    tracing::info!(
        "Update transaction confirmed with hash: {:?}",
        receipt.transaction_hash
    );

    // Check transaction status - only success if true
    if receipt.status() {
        tracing::info!("Update transaction succeeded (status: true)");
        Ok(tx_hash)
    } else {
        let error_msg = format!("Update transaction {tx_hash} reverted (status: false)");
        tracing::error!("{}", error_msg);
        tracing::error!("Receipt: {:?}", receipt);
        sentry::capture_message(&error_msg, sentry::Level::Error);
        Err(error_msg)
    }
}
