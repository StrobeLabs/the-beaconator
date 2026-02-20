use alloy::primitives::{Address, B256};
use alloy::providers::Provider;
use std::{str::FromStr, time::Duration};
use tokio::time::timeout;
use tracing;

use crate::models::beacon_type::{BeaconTypeConfig, FactoryType};
use crate::models::requests::BeaconCreationParams;
use crate::models::responses::CreateBeaconResponse;
use crate::models::{AppState, UpdateBeaconRequest};
use crate::routes::{IBeacon, IBeaconFactory, IBeaconRegistry};
use crate::services::beacon::verifiable::create_verifiable_beacon_with_factory;
use crate::services::transaction::events::{parse_beacon_created_event, parse_data_updated_event};
use crate::services::transaction::execution::is_nonce_error;

/// Create a beacon via the factory contract
///
/// This function handles:
/// - Wallet acquisition from WalletManager
/// - Transaction execution with error handling
/// - Transaction confirmation with progressive timeouts
/// - Event parsing to extract beacon address
///
/// The beacon owner will be set to the acquired wallet's address.
pub async fn create_beacon_via_factory(
    state: &AppState,
    factory_address: Address,
) -> Result<Address, String> {
    // Acquire a wallet from the pool
    let wallet_handle = state
        .wallet_manager
        .acquire_any_wallet()
        .await
        .map_err(|e| format!("Failed to acquire wallet: {e}"))?;

    let wallet_address = wallet_handle.address();
    // The beacon owner will be the wallet that creates it
    let owner_address = wallet_address;

    tracing::info!(
        "Creating beacon via factory {} for owner {}",
        factory_address,
        owner_address
    );
    tracing::info!("Acquired wallet {} for beacon creation", wallet_address);

    // Build provider with the acquired wallet
    let provider = wallet_handle
        .build_provider(&state.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;

    // Create contract instance using the wallet's provider
    let contract = IBeaconFactory::new(factory_address, &provider);

    // Send the beacon creation transaction
    tracing::info!("Creating beacon with wallet {}", wallet_address);
    let pending_tx = match contract.createBeacon(owner_address).send().await {
        Ok(pending) => Ok(pending),
        Err(e) => {
            let error_msg = format!("Failed to send createBeacon transaction: {e}");
            tracing::error!("{}", error_msg);

            // Check if nonce error
            if is_nonce_error(&error_msg) {
                tracing::warn!("Nonce error detected, transaction failed");
            }

            sentry::capture_message(&error_msg, sentry::Level::Error);
            Err(error_msg)
        }
    }?;

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

            // Try to get the receipt directly from the read provider with timeout
            match timeout(
                Duration::from_secs(30),
                state.read_provider.get_transaction_receipt(tx_hash),
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

    match state.read_provider.get_transaction_receipt(tx_hash).await {
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

    // Create contract instance and call beacons(address) directly using read provider
    let contract = IBeaconRegistry::new(registry_address, &*state.read_provider);

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
/// - Wallet acquisition from WalletManager
/// - Transaction execution with error handling
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
    match state.read_provider.get_code_at(beacon_address).await {
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

    // Acquire a wallet from the pool
    let wallet_handle = state
        .wallet_manager
        .acquire_any_wallet()
        .await
        .map_err(|e| format!("Failed to acquire wallet: {e}"))?;

    let wallet_address = wallet_handle.address();
    tracing::info!("Acquired wallet {} for beacon registration", wallet_address);

    // Build provider with the acquired wallet
    let provider = wallet_handle
        .build_provider(&state.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;

    // Create contract instance using the wallet's provider
    let contract = IBeaconRegistry::new(registry_address, &provider);

    // Send the registration transaction
    tracing::info!("Registering beacon with wallet {}", wallet_address);
    let pending_tx = match contract.registerBeacon(beacon_address).send().await {
        Ok(pending) => Ok(pending),
        Err(e) => {
            let error_msg = format!("Failed to send registerBeacon transaction: {e}");
            tracing::error!("{}", error_msg);

            // Check if nonce error
            if is_nonce_error(&error_msg) {
                tracing::warn!("Nonce error detected, transaction failed");
            }

            sentry::capture_message(&error_msg, sentry::Level::Error);
            Err(error_msg)
        }
    }?;

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

            // Try to get the receipt directly from the read provider with timeout
            match timeout(
                Duration::from_secs(30),
                state.read_provider.get_transaction_receipt(tx_hash),
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
/// - Wallet acquisition from WalletManager
/// - Transaction execution with error handling
/// - Transaction confirmation with timeouts
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

    // proof and public_signals are already Bytes (from 0x-hex JSON)
    let proof_bytes = request.proof;
    let public_signals_bytes = request.public_signals;

    // Acquire a wallet from the pool (prefer wallet designated for this beacon)
    let wallet_handle = state
        .wallet_manager
        .acquire_for_beacon(&beacon_address)
        .await
        .map_err(|e| format!("Failed to acquire wallet: {e}"))?;

    let wallet_address = wallet_handle.address();
    tracing::info!("Acquired wallet {} for beacon update", wallet_address);

    // Build provider with the acquired wallet
    let provider = wallet_handle
        .build_provider(&state.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;

    // Create contract instance using the wallet's provider
    let contract = IBeacon::new(beacon_address, &provider);

    // Send the update transaction
    tracing::info!("Updating beacon with wallet {}", wallet_address);
    let pending_tx = match contract
        .updateData(proof_bytes.clone(), public_signals_bytes.clone())
        .send()
        .await
    {
        Ok(pending) => Ok(pending),
        Err(e) => {
            let error_msg = format!("Failed to send updateData transaction: {e}");
            tracing::error!("{}", error_msg);

            // Check if nonce error
            if is_nonce_error(&error_msg) {
                tracing::warn!("Nonce error detected, transaction failed");
            }

            sentry::capture_message(&error_msg, sentry::Level::Error);
            Err(error_msg)
        }
    }?;

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

            // Try to get the receipt directly from the read provider with timeout
            match timeout(
                Duration::from_secs(30),
                state.read_provider.get_transaction_receipt(tx_hash),
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

    // First check transaction status
    if !receipt.status() {
        let error_msg = format!("Update transaction {tx_hash} reverted (status: false)");
        tracing::error!("{}", error_msg);
        tracing::error!("Receipt: {:?}", receipt);
        sentry::capture_message(&error_msg, sentry::Level::Error);
        return Err(error_msg);
    }

    // Parse and validate DataUpdated event was emitted
    match parse_data_updated_event(&receipt, beacon_address) {
        Ok(new_data) => {
            tracing::info!(
                "Update transaction succeeded - beacon {} updated to data: {}",
                beacon_address,
                new_data
            );
            Ok(tx_hash)
        }
        Err(e) => {
            let error_msg = format!(
                "Transaction succeeded but DataUpdated event not found: {e}. This indicates the update may not have been applied."
            );
            tracing::error!("{}", error_msg);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            Err(error_msg)
        }
    }
}

/// Dispatch beacon creation to the correct factory based on FactoryType.
///
/// For Simple factories, calls create_beacon_via_factory().
/// For Dichotomous factories, validates required params and calls create_verifiable_beacon_with_factory().
pub async fn create_beacon_by_type(
    state: &AppState,
    config: &BeaconTypeConfig,
    params: Option<&BeaconCreationParams>,
) -> Result<Address, String> {
    match config.factory_type {
        FactoryType::Simple => create_beacon_via_factory(state, config.factory_address).await,
        FactoryType::Dichotomous => {
            let params = params.ok_or(
                "Dichotomous factory type requires params (verifier_address, initial_data, initial_cardinality)"
            )?;
            let verifier_str = params
                .verifier_address
                .as_ref()
                .ok_or("verifier_address is required for Dichotomous factory type")?;
            let verifier_address = Address::from_str(verifier_str)
                .map_err(|e| format!("Invalid verifier_address: {e}"))?;
            let initial_data = params
                .initial_data
                .ok_or("initial_data is required for Dichotomous factory type")?;
            let initial_cardinality = params
                .initial_cardinality
                .ok_or("initial_cardinality is required for Dichotomous factory type")?;

            create_verifiable_beacon_with_factory(
                state,
                config.factory_address,
                verifier_address,
                initial_data,
                initial_cardinality,
            )
            .await
        }
    }
}

/// Create a beacon by type and optionally register it with the configured registry.
pub async fn create_and_register_beacon_by_type(
    state: &AppState,
    config: &BeaconTypeConfig,
    params: Option<&BeaconCreationParams>,
) -> Result<CreateBeaconResponse, String> {
    let beacon_address = create_beacon_by_type(state, config, params).await?;

    let registered = if let Some(registry_address) = config.registry_address {
        match register_beacon_with_registry(state, beacon_address, registry_address).await {
            Ok(_) => {
                tracing::info!(
                    "Beacon {} registered with registry {}",
                    beacon_address,
                    registry_address
                );
                true
            }
            Err(e) => {
                tracing::warn!(
                    "Beacon {} created but registration failed: {}",
                    beacon_address,
                    e
                );
                false
            }
        }
    } else {
        false
    };

    Ok(CreateBeaconResponse {
        beacon_address: format!("{beacon_address:#x}"),
        beacon_type: config.slug.clone(),
        factory_address: format!("{:#x}", config.factory_address),
        registered,
    })
}
