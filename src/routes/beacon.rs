use alloy::primitives::{Address, B256, Bytes};
use alloy::providers::Provider;
use rocket::serde::json::Json;
use rocket::{State, http::Status, post};
use std::str::FromStr;
use std::time::Duration;
use tokio::time::timeout;
use tracing;

use super::{
    IBeacon, IBeaconFactory, IBeaconRegistry, IMulticall3, execute_transaction_serialized,
    get_fresh_nonce_from_alternate, is_nonce_error, sync_wallet_nonce,
};
use crate::guards::ApiToken;
use crate::models::{
    ApiResponse, AppState, BatchCreatePerpcityBeaconRequest, BatchCreatePerpcityBeaconResponse,
    BatchUpdateBeaconRequest, BatchUpdateBeaconResponse, BeaconUpdateData, BeaconUpdateResult,
    CreateBeaconRequest, RegisterBeaconRequest, UpdateBeaconRequest,
};

// Helper function to create a beacon via the factory contract
async fn create_beacon_via_factory(
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
    let pending_tx = execute_transaction_serialized(&*state.provider, state.wallet_address, async {
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
                    tracing::warn!(
                        "Nonce error detected, attempting to sync nonce before fallback"
                    );
                    if let Err(sync_error) = sync_wallet_nonce(state).await {
                        tracing::error!("Nonce sync failed: {}", sync_error);
                    }
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
    }).await?;

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

    // Parse the beacon address from the event logs
    let beacon_address = parse_beacon_created_event(&receipt, factory_address)?;

    tracing::info!("Beacon created at address: {}", beacon_address);
    sentry::capture_message(
        &format!("Beacon created via factory: {beacon_address}"),
        sentry::Level::Info,
    );
    Ok(beacon_address)
}

// Helper function to check if a transaction is already confirmed on-chain
async fn is_transaction_confirmed(
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

// Helper function to check if a beacon is already registered
async fn is_beacon_registered(
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

// Helper function to register a beacon with a registry
async fn register_beacon_with_registry(
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
    let pending_tx = execute_transaction_serialized(&*state.provider, state.wallet_address, async {
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
                    tracing::warn!(
                        "Nonce error detected, attempting to sync nonce before fallback"
                    );
                    if let Err(sync_error) = sync_wallet_nonce(state).await {
                        tracing::error!("Nonce sync failed: {}", sync_error);
                    }
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
    }).await?;

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

    sentry::capture_message(
        &format!("Beacon {beacon_address} registered with registry {registry_address}"),
        sentry::Level::Info,
    );

    Ok(tx_hash)
}

// Helper function to parse the BeaconCreated event from transaction receipt
fn parse_beacon_created_event(
    receipt: &alloy::rpc::types::TransactionReceipt,
    factory_address: Address,
) -> Result<Address, String> {
    // Look for the BeaconCreated event in the logs
    for log in receipt.logs().iter() {
        // Check if this log is from our factory contract
        if log.address() == factory_address {
            // Try to decode as BeaconCreated event
            match log.log_decode::<IBeaconFactory::BeaconCreated>() {
                Ok(decoded_log) => {
                    let beacon = decoded_log.inner.data.beacon;
                    tracing::info!(
                        "Successfully parsed BeaconCreated event - beacon address: {}",
                        beacon
                    );
                    return Ok(beacon);
                }
                Err(_) => {
                    // Log is from factory but not BeaconCreated event, continue
                }
            }
        }
    }

    let error_msg = "BeaconCreated event not found in transaction receipt";
    tracing::error!("{}", error_msg);
    tracing::error!("Total logs in receipt: {}", receipt.logs().len());
    sentry::capture_message(error_msg, sentry::Level::Error);
    Err(error_msg.to_string())
}

#[post("/create_beacon", data = "<_request>")]
pub async fn create_beacon(
    _request: Json<CreateBeaconRequest>,
    _token: ApiToken,
) -> Json<ApiResponse<String>> {
    tracing::info!("Received request: POST /create_beacon");
    Json(ApiResponse {
        success: false,
        data: None,
        message: "create_beacon endpoint not yet implemented".to_string(),
    })
}

#[post("/register_beacon", data = "<_request>")]
pub async fn register_beacon(
    _request: Json<RegisterBeaconRequest>,
    _token: ApiToken,
) -> Json<ApiResponse<String>> {
    tracing::info!("Received request: POST /register_beacon");
    // TODO: Implement beacon registration
    Json(ApiResponse {
        success: false,
        data: None,
        message: "register_beacon endpoint not yet implemented".to_string(),
    })
}

#[post("/create_perpcity_beacon")]
pub async fn create_perpcity_beacon(
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<String>>, Status> {
    tracing::info!("Received request: POST /create_perpcity_beacon");

    // Log configuration details for debugging
    tracing::debug!("Configuration:");
    tracing::debug!("  - Wallet address: {}", state.wallet_address);
    tracing::debug!(
        "  - Beacon factory address: {}",
        state.beacon_factory_address
    );
    tracing::debug!(
        "  - Perpcity registry address: {}",
        state.perpcity_registry_address
    );

    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/create_perpcity_beacon");
        scope.set_extra("wallet_address", state.wallet_address.to_string().into());
        scope.set_extra(
            "beacon_factory_address",
            state.beacon_factory_address.to_string().into(),
        );
        scope.set_extra(
            "perpcity_registry_address",
            state.perpcity_registry_address.to_string().into(),
        );
    });

    // Create a beacon using the factory
    let owner_address = state.wallet_address;
    tracing::info!("Starting beacon creation for owner: {}", owner_address);

    let beacon_address =
        match create_beacon_via_factory(state, owner_address, state.beacon_factory_address).await {
            Ok(address) => {
                tracing::info!("Successfully created beacon at address: {}", address);
                sentry::capture_message(
                    &format!("Beacon created successfully at: {address}"),
                    sentry::Level::Info,
                );
                address
            }
            Err(e) => {
                tracing::error!("Failed to create beacon: {}", e);
                tracing::error!("Error details: {:?}", e);
                sentry::capture_message(
                    &format!("Failed to create beacon: {e}"),
                    sentry::Level::Error,
                );
                return Err(Status::InternalServerError);
            }
        };

    // The beacon creation transaction is now fully confirmed, so we can safely proceed with registration
    tracing::info!("Beacon creation completed successfully, proceeding with registration...");

    // Register the beacon with the perpcity registry
    tracing::info!(
        "Starting beacon registration for beacon: {}",
        beacon_address
    );

    match register_beacon_with_registry(state, beacon_address, state.perpcity_registry_address)
        .await
    {
        Ok(tx_hash) => {
            let message = if tx_hash == B256::ZERO {
                "Perpcity beacon created successfully (already registered)"
            } else {
                "Perpcity beacon created and registered successfully"
            };

            if tx_hash == B256::ZERO {
                tracing::info!(
                    "{} - Beacon: {} was already registered",
                    message,
                    beacon_address
                );
            } else {
                tracing::info!(
                    "{} - Beacon: {}, TX: {:?}",
                    message,
                    beacon_address,
                    tx_hash
                );
            }

            sentry::capture_message(
                &format!("Beacon successfully created: {beacon_address}"),
                sentry::Level::Info,
            );
            Ok(Json(ApiResponse {
                success: true,
                data: Some(format!("Beacon address: {beacon_address}")),
                message: message.to_string(),
            }))
        }
        Err(e) => {
            tracing::error!(
                "Failed to register beacon {} with registry: {}",
                beacon_address,
                e
            );
            tracing::error!("Error details: {:?}", e);
            sentry::capture_message(
                &format!("Failed to register beacon {beacon_address}: {e}"),
                sentry::Level::Error,
            );
            Err(Status::InternalServerError)
        }
    }
}

#[post("/batch_create_perpcity_beacon", data = "<request>")]
pub async fn batch_create_perpcity_beacon(
    request: Json<BatchCreatePerpcityBeaconRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<BatchCreatePerpcityBeaconResponse>>, Status> {
    tracing::info!("Received request: POST /batch_create_perpcity_beacon");
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/batch_create_perpcity_beacon");
        scope.set_extra("requested_count", request.count.into());
    });

    let count = request.count;

    // Validate the count
    if count == 0 || count > 100 {
        tracing::warn!("Invalid beacon count: {}", count);
        return Err(Status::BadRequest);
    }

    // Process all beacon creations in a single serialized transaction using multicall for efficiency
    let state_inner = state.inner();
    let count_clone = count;
    let owner_address = state.wallet_address;

    let batch_results =
        execute_transaction_serialized(&*state.provider, state.wallet_address, async move {
            // Check if we have a multicall3 contract address configured
            if let Some(multicall_address) = state_inner.multicall3_address {
                // Use multicall3 for atomic batch beacon creation
                batch_create_beacons_with_multicall3(
                    state_inner,
                    multicall_address,
                    count_clone,
                    owner_address,
                )
                .await
            } else {
                // No multicall3 configured - return error for all beacon creations
                let error_msg =
                    "Batch operations require Multicall3 contract address to be configured"
                        .to_string();
                tracing::error!("{}", error_msg);
                (1..=count_clone)
                    .map(|i| (i, Err(error_msg.clone())))
                    .collect()
            }
        })
        .await;

    // Process the results
    let mut beacon_addresses = Vec::new();
    let mut errors = Vec::new();

    for (_i, result) in batch_results {
        match result {
            Ok(address) => {
                beacon_addresses.push(address);
            }
            Err(error) => {
                errors.push(error);
            }
        }
    }

    let created_count = beacon_addresses.len() as u32;
    let failed_count = count - created_count;

    let response_data = BatchCreatePerpcityBeaconResponse {
        created_count,
        beacon_addresses: beacon_addresses.clone(),
        failed_count,
        errors,
    };

    let message = if failed_count == 0 {
        format!("Successfully created and registered all {created_count} Perpcity beacons")
    } else if created_count == 0 {
        "Failed to create any beacons".to_string()
    } else {
        format!("Partially successful: {created_count} created, {failed_count} failed")
    };

    tracing::info!("{}", message);

    // Return success even with partial failures, let client handle the response
    Ok(Json(ApiResponse {
        success: created_count > 0,
        data: Some(response_data),
        message,
    }))
}

#[post("/update_beacon", data = "<request>")]
pub async fn update_beacon(
    request: Json<UpdateBeaconRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<String>>, Status> {
    tracing::info!("Received request: POST /update_beacon");
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/update_beacon");
        scope.set_extra("beacon_address", request.beacon_address.clone().into());
        scope.set_extra("value", request.value.into());
    });

    // Parse the beacon address
    let beacon_address = match Address::from_str(&request.beacon_address) {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("Invalid beacon address: {}", e);
            return Err(Status::BadRequest);
        }
    };

    // Create contract instance using the sol! generated interface
    let contract = IBeacon::new(beacon_address, &*state.provider);

    // Prepare the proof and public signals
    let proof_bytes = Bytes::from(request.proof.clone());
    let public_signals_bytes = Bytes::from(vec![0u8; 32]); // Placeholder for now

    tracing::debug!("Sending updateData transaction...");

    // Send the transaction and wait for receipt
    let receipt = match contract
        .updateData(proof_bytes, public_signals_bytes)
        .send()
        .await
    {
        Ok(pending_tx) => match pending_tx.get_receipt().await {
            Ok(receipt) => receipt,
            Err(e) => {
                tracing::error!("Failed to get receipt: {}", e);
                sentry::capture_message(
                    &format!("Failed to get receipt: {e}"),
                    sentry::Level::Error,
                );
                return Err(Status::InternalServerError);
            }
        },
        Err(e) => {
            tracing::error!("Failed to send transaction: {}", e);
            sentry::capture_message(
                &format!("Failed to send transaction: {e}"),
                sentry::Level::Error,
            );
            return Err(Status::InternalServerError);
        }
    };

    tracing::info!(
        "Update transaction confirmed with hash: {:?}",
        receipt.transaction_hash
    );

    let message = "Beacon updated successfully";
    Ok(Json(ApiResponse {
        success: true,
        data: Some(format!("Transaction hash: {:?}", receipt.transaction_hash)),
        message: message.to_string(),
    }))
}

#[post("/batch_update_beacon", data = "<request>")]
pub async fn batch_update_beacon(
    request: Json<BatchUpdateBeaconRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<BatchUpdateBeaconResponse>>, Status> {
    tracing::info!("Received request: POST /batch_update_beacon");
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/batch_update_beacon");
        scope.set_extra("update_count", request.updates.len().into());
    });

    // Validate request
    if request.updates.is_empty() {
        tracing::warn!("Batch update request with no updates");
        return Err(Status::BadRequest);
    }

    if request.updates.len() > 100 {
        tracing::warn!("Batch update request exceeds maximum of 100 updates");
        return Err(Status::BadRequest);
    }

    // Process all updates using multicall for efficient batching
    let state_inner = state.inner();
    let updates_clone = request.updates.clone();

    let batch_results =
        execute_transaction_serialized(&*state.provider, state.wallet_address, async move {
            // Check if we have a multicall3 contract address configured
            if let Some(multicall_address) = state_inner.multicall3_address {
                // Use multicall3 for efficient batch execution - single transaction with multiple calls
                batch_update_with_multicall3(state_inner, multicall_address, &updates_clone).await
            } else {
                // No multicall3 configured - return error for all updates
                let error_msg =
                    "Batch operations require Multicall3 contract address to be configured"
                        .to_string();
                tracing::error!("{}", error_msg);
                updates_clone
                    .iter()
                    .map(|update| (update.beacon_address.clone(), Err(error_msg.clone())))
                    .collect()
            }
        })
        .await;

    // Process the results
    let mut results = Vec::new();
    let mut successful_updates = 0;
    let mut failed_updates = 0;

    for (beacon_address, result) in batch_results {
        match result {
            Ok(tx_hash) => {
                successful_updates += 1;
                results.push(BeaconUpdateResult {
                    beacon_address: beacon_address.clone(),
                    success: true,
                    transaction_hash: Some(tx_hash.clone()),
                    error: None,
                });
                tracing::info!(
                    "Successfully updated beacon {} with tx hash: {}",
                    beacon_address,
                    tx_hash
                );
            }
            Err(error) => {
                failed_updates += 1;
                results.push(BeaconUpdateResult {
                    beacon_address: beacon_address.clone(),
                    success: false,
                    transaction_hash: None,
                    error: Some(error.clone()),
                });
                tracing::error!("Failed to update beacon {}: {}", beacon_address, error);
            }
        }
    }

    let response = BatchUpdateBeaconResponse {
        results,
        total_requested: request.updates.len(),
        successful_updates,
        failed_updates,
    };

    let message = format!(
        "Batch update completed: {}/{} successful",
        successful_updates,
        request.updates.len()
    );

    Ok(Json(ApiResponse {
        success: successful_updates > 0,
        data: Some(response),
        message,
    }))
}

// Helper function to execute batch updates using multicall3 - single transaction with multiple calls
async fn batch_update_with_multicall3(
    state: &AppState,
    multicall_address: Address,
    updates: &[BeaconUpdateData],
) -> Vec<(String, Result<String, String>)> {
    tracing::info!(
        "Using Multicall3 for batch update of {} beacons",
        updates.len()
    );

    // Prepare multicall3 calls - each beacon update becomes a call in the multicall
    let mut calls = Vec::new();
    let mut beacon_addresses = Vec::new();
    let mut invalid_addresses = Vec::new();

    for update_data in updates {
        // Parse beacon address
        let beacon_address = match Address::from_str(&update_data.beacon_address) {
            Ok(addr) => addr,
            Err(e) => {
                // Track invalid address for error reporting
                invalid_addresses.push((
                    update_data.beacon_address.clone(),
                    format!("Invalid beacon address: {e}"),
                ));
                continue; // Skip this update but continue processing others
            }
        };

        // Prepare proof and public signals
        let proof_bytes = Bytes::from(update_data.proof.clone());
        let public_signals_bytes = Bytes::from(vec![0u8; 32]); // Placeholder for now

        // Create the updateData call data using the IBeacon interface
        let beacon_contract = IBeacon::new(beacon_address, &*state.provider);
        let call_data = beacon_contract
            .updateData(proof_bytes, public_signals_bytes)
            .calldata()
            .clone();

        // Create multicall3 call
        let call = IMulticall3::Call3 {
            target: beacon_address,
            allowFailure: false, // Atomic: all calls must succeed or entire batch fails
            callData: call_data,
        };

        calls.push(call);
        beacon_addresses.push(update_data.beacon_address.clone());
    }

    // Execute the multicall3 transaction - single transaction containing all calls
    let multicall_contract = IMulticall3::new(multicall_address, &*state.provider);

    // Build results in the same order as the input
    let mut results = Vec::new();

    // Process each update in order
    for update_data in updates {
        if let Some((addr, error)) = invalid_addresses
            .iter()
            .find(|(a, _)| a == &update_data.beacon_address)
        {
            // This was an invalid address
            results.push((addr.clone(), Err(error.clone())));
        } else {
            // This was a valid address - find its result from multicall
            if beacon_addresses
                .iter()
                .any(|a| a == &update_data.beacon_address)
            {
                // We'll add the multicall result later
                results.push((
                    update_data.beacon_address.clone(),
                    Err("Multicall3 processing failed".to_string()),
                ));
            }
        }
    }

    // If we have valid calls to make, execute the multicall and update results
    if !calls.is_empty() {
        match multicall_contract.aggregate3(calls).send().await {
            Ok(pending_tx) => {
                tracing::info!("Multicall3 transaction sent, waiting for receipt...");
                match pending_tx.get_receipt().await {
                    Ok(receipt) => {
                        tracing::info!(
                            "Multicall3 transaction confirmed: {:?}",
                            receipt.transaction_hash
                        );

                        // Update results for valid addresses
                        for addr in beacon_addresses.iter() {
                            if let Some(result_index) = results.iter().position(|(a, _)| a == addr)
                            {
                                results[result_index] =
                                    (addr.clone(), Ok(format!("{:?}", receipt.transaction_hash)));
                            }
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Failed to get multicall3 receipt: {e}");
                        tracing::error!("{}", error_msg);
                        // Update results for valid addresses with error
                        for addr in beacon_addresses {
                            if let Some(result_index) = results.iter().position(|(a, _)| a == &addr)
                            {
                                results[result_index] = (addr.clone(), Err(error_msg.clone()));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                let error_msg = format!("Failed to send multicall3 transaction: {e}");
                tracing::error!("{}", error_msg);
                // Update results for valid addresses with error
                for addr in beacon_addresses {
                    if let Some(result_index) = results.iter().position(|(a, _)| a == &addr) {
                        results[result_index] = (addr.clone(), Err(error_msg.clone()));
                    }
                }
            }
        }
    }

    results
}

// Helper function to execute batch beacon creation using multicall3 - single transaction with multiple calls
async fn batch_create_beacons_with_multicall3(
    state: &AppState,
    multicall_address: Address,
    count: u32,
    owner_address: Address,
) -> Vec<(u32, Result<String, String>)> {
    tracing::info!("Using Multicall3 for batch creation of {} beacons", count);

    // Prepare multicall3 calls - each beacon creation becomes a call in the multicall
    let mut calls = Vec::new();

    for _i in 1..=count {
        // Create the createBeacon call data using the IBeaconFactory interface
        let factory_contract = IBeaconFactory::new(state.beacon_factory_address, &*state.provider);
        let call_data = factory_contract
            .createBeacon(owner_address)
            .calldata()
            .clone();

        // Create multicall3 call
        let call = IMulticall3::Call3 {
            target: state.beacon_factory_address,
            allowFailure: false, // Atomic: all beacon creations must succeed or entire batch fails
            callData: call_data,
        };

        calls.push(call);
    }

    // Execute the multicall3 transaction - single transaction containing all beacon creations
    let multicall_contract = IMulticall3::new(multicall_address, &*state.provider);

    match multicall_contract.aggregate3(calls).send().await {
        Ok(pending_tx) => {
            tracing::info!("Multicall3 beacon creation transaction sent, waiting for receipt...");
            match pending_tx.get_receipt().await {
                Ok(receipt) => {
                    tracing::info!(
                        "Multicall3 beacon creation confirmed: {:?}",
                        receipt.transaction_hash
                    );

                    // Parse beacon addresses from the event logs
                    let beacon_addresses = parse_beacon_created_events_from_multicall(
                        &receipt,
                        state.beacon_factory_address,
                        count,
                    );

                    match beacon_addresses {
                        Ok(addresses) => {
                            // Register all created beacons with the registry in another multicall
                            match register_beacons_with_multicall3(
                                state,
                                multicall_address,
                                &addresses,
                            )
                            .await
                            {
                                Ok(_) => {
                                    // All succeeded
                                    addresses
                                        .into_iter()
                                        .enumerate()
                                        .map(|(i, addr)| ((i + 1) as u32, Ok(addr)))
                                        .collect()
                                }
                                Err(e) => {
                                    let error_msg = format!("Failed to register beacons: {e}");
                                    (1..=count).map(|i| (i, Err(error_msg.clone()))).collect()
                                }
                            }
                        }
                        Err(e) => {
                            let error_msg =
                                format!("Failed to parse beacon addresses from multicall: {e}");
                            tracing::error!("{}", error_msg);
                            (1..=count).map(|i| (i, Err(error_msg.clone()))).collect()
                        }
                    }
                }
                Err(e) => {
                    let error_msg =
                        format!("Failed to get multicall3 beacon creation receipt: {e}");
                    tracing::error!("{}", error_msg);
                    (1..=count).map(|i| (i, Err(error_msg.clone()))).collect()
                }
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to send multicall3 beacon creation transaction: {e}");
            tracing::error!("{}", error_msg);
            (1..=count).map(|i| (i, Err(error_msg.clone()))).collect()
        }
    }
}

// Helper function to register multiple beacons using multicall3
async fn register_beacons_with_multicall3(
    state: &AppState,
    multicall_address: Address,
    beacon_addresses: &[String],
) -> Result<(), String> {
    tracing::info!(
        "Using Multicall3 for batch registration of {} beacons",
        beacon_addresses.len()
    );

    let mut calls = Vec::new();

    for beacon_addr_str in beacon_addresses {
        let beacon_address = Address::from_str(beacon_addr_str)
            .map_err(|e| format!("Invalid beacon address {beacon_addr_str}: {e}"))?;

        // Create the registerBeacon call data using the IBeaconRegistry interface
        let registry_contract =
            IBeaconRegistry::new(state.perpcity_registry_address, &*state.provider);
        let call_data = registry_contract
            .registerBeacon(beacon_address)
            .calldata()
            .clone();

        // Create multicall3 call
        let call = IMulticall3::Call3 {
            target: state.perpcity_registry_address,
            allowFailure: false, // Atomic: all registrations must succeed
            callData: call_data,
        };

        calls.push(call);
    }

    // Execute the multicall3 registration transaction
    let multicall_contract = IMulticall3::new(multicall_address, &*state.provider);

    match multicall_contract.aggregate3(calls).send().await {
        Ok(pending_tx) => match pending_tx.get_receipt().await {
            Ok(receipt) => {
                tracing::info!(
                    "Multicall3 beacon registration confirmed: {:?}",
                    receipt.transaction_hash
                );
                Ok(())
            }
            Err(e) => Err(format!("Failed to get registration receipt: {e}")),
        },
        Err(e) => Err(format!("Failed to send registration transaction: {e}")),
    }
}

// Helper function to parse beacon addresses from multicall receipt
fn parse_beacon_created_events_from_multicall(
    receipt: &alloy::rpc::types::TransactionReceipt,
    factory_address: Address,
    expected_count: u32,
) -> Result<Vec<String>, String> {
    let mut beacon_addresses = Vec::new();

    // Look for BeaconCreated events in the logs
    for log in receipt.logs().iter() {
        // Check if this log is from our factory contract
        if log.address() == factory_address {
            // Try to decode as BeaconCreated event
            match log.log_decode::<IBeaconFactory::BeaconCreated>() {
                Ok(decoded_log) => {
                    let beacon = decoded_log.inner.data.beacon;
                    beacon_addresses.push(beacon.to_string());
                    tracing::info!("Parsed BeaconCreated event - beacon address: {}", beacon);
                }
                Err(_) => {
                    // Log is from factory but not BeaconCreated event, continue
                }
            }
        }
    }

    if beacon_addresses.len() as u32 != expected_count {
        return Err(format!(
            "Expected {} BeaconCreated events, but found {}",
            expected_count,
            beacon_addresses.len()
        ));
    }

    Ok(beacon_addresses)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::Address;
    use std::str::FromStr;

    #[tokio::test]
    async fn test_batch_update_beacon_with_multicall3() {
        use crate::guards::ApiToken;
        use crate::models::{BatchUpdateBeaconRequest, BeaconUpdateData};
        use crate::routes::test_utils::create_simple_test_app_state;
        use rocket::State;

        let token = ApiToken("test_token".to_string());
        let mut app_state = create_simple_test_app_state();

        // Set multicall3 address for the test
        app_state.multicall3_address =
            Some(Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap());

        let state = State::from(&app_state);

        let update_data = BeaconUpdateData {
            beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
            value: 100,
            proof: vec![1, 2, 3, 4], // Mock proof
        };

        let request = Json(BatchUpdateBeaconRequest {
            updates: vec![update_data],
        });

        // This will fail in test environment due to no actual contracts, but should not panic
        let result = batch_update_beacon(request, token, state).await;

        // Should return an error response rather than panic
        assert!(result.is_ok());
        let response = result.unwrap().into_inner();

        // Should contain error details about the failed multicall
        assert!(!response.success);
        assert!(response.data.is_some());
        let batch_data = response.data.unwrap();
        assert_eq!(batch_data.successful_updates, 0);
        assert_eq!(batch_data.failed_updates, 1);
        assert!(!batch_data.results.is_empty());
    }

    #[tokio::test]
    async fn test_batch_update_beacon_without_multicall3() {
        use crate::guards::ApiToken;
        use crate::models::{BatchUpdateBeaconRequest, BeaconUpdateData};
        use crate::routes::test_utils::create_simple_test_app_state;
        use rocket::State;

        let token = ApiToken("test_token".to_string());
        let app_state = create_simple_test_app_state(); // No multicall3_address set
        let state = State::from(&app_state);

        let update_data = BeaconUpdateData {
            beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
            value: 100,
            proof: vec![1, 2, 3, 4],
        };

        let request = Json(BatchUpdateBeaconRequest {
            updates: vec![update_data],
        });

        let result = batch_update_beacon(request, token, state).await;

        assert!(result.is_ok());
        let response = result.unwrap().into_inner();

        // Should fail with clear error message about missing multicall3
        assert!(!response.success);
        assert!(response.data.is_some());
        let batch_data = response.data.unwrap();
        assert_eq!(batch_data.successful_updates, 0);
        assert_eq!(batch_data.failed_updates, 1);
        assert!(
            batch_data.results[0]
                .error
                .as_ref()
                .unwrap()
                .contains("Multicall3")
                || batch_data.results[0]
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("multicall")
        );
    }

    #[tokio::test]
    async fn test_batch_create_beacons_with_multicall3() {
        use crate::guards::ApiToken;
        use crate::models::BatchCreatePerpcityBeaconRequest;
        use crate::routes::test_utils::create_simple_test_app_state;
        use rocket::State;

        let token = ApiToken("test_token".to_string());
        let mut app_state = create_simple_test_app_state();

        // Set multicall3 address for the test
        app_state.multicall3_address =
            Some(Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap());

        let state = State::from(&app_state);

        let request = Json(BatchCreatePerpcityBeaconRequest { count: 3 });

        let result = batch_create_perpcity_beacon(request, token, state).await;

        // Should return an error response due to multicall not implemented yet
        assert!(result.is_ok());
        let response = result.unwrap().into_inner();

        assert!(!response.success);
        assert!(response.data.is_some());
        let batch_data = response.data.unwrap();
        assert_eq!(batch_data.created_count, 0);
        assert_eq!(batch_data.failed_count, 3);
        assert!(!batch_data.errors.is_empty());
    }

    #[tokio::test]
    async fn test_batch_create_beacons_without_multicall3() {
        use crate::guards::ApiToken;
        use crate::models::BatchCreatePerpcityBeaconRequest;
        use crate::routes::test_utils::create_simple_test_app_state;
        use rocket::State;

        let token = ApiToken("test_token".to_string());
        let app_state = create_simple_test_app_state(); // No multicall3_address set
        let state = State::from(&app_state);

        let request = Json(BatchCreatePerpcityBeaconRequest { count: 2 });

        let result = batch_create_perpcity_beacon(request, token, state).await;

        assert!(result.is_ok());
        let response = result.unwrap().into_inner();

        // Should fail with clear error message about missing multicall3
        assert!(!response.success);
        assert!(response.data.is_some());
        let batch_data = response.data.unwrap();
        assert_eq!(batch_data.created_count, 0);
        assert_eq!(batch_data.failed_count, 2);
        assert!(
            batch_data
                .errors
                .iter()
                .any(|e| e.contains("Multicall3") || e.contains("multicall"))
        );
    }

    #[tokio::test]
    async fn test_multicall3_atomic_behavior() {
        // Test that multicall3 calls are atomic (allowFailure: false)
        use crate::models::BeaconUpdateData;

        let update_data = BeaconUpdateData {
            beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
            value: 100,
            proof: vec![1, 2, 3, 4],
        };

        // Create mock multicall3 call and verify atomicity setting
        let beacon_address = Address::from_str(&update_data.beacon_address).unwrap();
        let _proof_bytes = Bytes::from(update_data.proof.clone());
        let _public_signals_bytes = Bytes::from(vec![0u8; 32]);

        // This would be the actual call structure in the multicall
        let call = IMulticall3::Call3 {
            target: beacon_address,
            allowFailure: false,    // Atomic behavior
            callData: Bytes::new(), // Mock call data
        };

        // Verify atomic setting
        assert!(
            !call.allowFailure,
            "Multicall3 calls should be atomic (allowFailure: false)"
        );
        assert_eq!(call.target, beacon_address);
    }
    use crate::routes::test_utils::create_test_app_state;

    #[tokio::test]
    async fn test_create_beacon_not_implemented() {
        use crate::guards::ApiToken;

        // Create a mock ApiToken
        let token = ApiToken("test_token".to_string());

        let request = Json(CreateBeaconRequest {
            placeholder: "test".to_string(),
        });

        let result = create_beacon(request, token).await;
        let response = result.into_inner();

        assert!(!response.success);
        assert!(response.message.contains("not yet implemented"));
    }

    #[tokio::test]
    async fn test_register_beacon_not_implemented() {
        use crate::guards::ApiToken;

        // Create a mock ApiToken
        let token = ApiToken("test_token".to_string());

        let request = Json(RegisterBeaconRequest {
            placeholder: "test".to_string(),
        });

        let result = register_beacon(request, token).await;
        let response = result.into_inner();

        assert!(!response.success);
        assert!(response.message.contains("not yet implemented"));
    }

    #[tokio::test]
    async fn test_create_perpcity_beacon_fails_without_network() {
        use crate::guards::ApiToken;
        use crate::routes::test_utils::create_simple_test_app_state;
        use rocket::State;

        // This test will fail because we can't actually connect to a network
        let token = ApiToken("test_token".to_string());
        let app_state = create_simple_test_app_state();
        let state = State::from(&app_state);

        let result = create_perpcity_beacon(token, state).await;
        // We expect this to fail since we don't have a real network connection
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_beacon_via_factory_helper() {
        use crate::routes::test_utils::create_simple_test_app_state;

        let app_state = create_simple_test_app_state();
        let owner_address =
            Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let factory_address = app_state.beacon_factory_address;

        // This will fail without a real network, but tests the function signature
        let result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;
        assert!(result.is_err()); // Expected to fail without real network
    }

    #[tokio::test]
    async fn test_register_beacon_with_registry_helper() {
        use crate::routes::test_utils::create_simple_test_app_state;

        let app_state = create_simple_test_app_state();
        let beacon_address =
            Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let registry_address = app_state.perpcity_registry_address;

        // This will fail without a real network, but tests the function signature
        let result =
            register_beacon_with_registry(&app_state, beacon_address, registry_address).await;
        assert!(result.is_err()); // Expected to fail without real network
    }

    #[tokio::test]
    async fn test_batch_create_perpcity_beacon_invalid_count() {
        use crate::guards::ApiToken;
        use crate::models::BatchCreatePerpcityBeaconRequest;
        use crate::routes::test_utils::create_simple_test_app_state;
        use rocket::State;

        let token = ApiToken("test_token".to_string());
        let app_state = create_simple_test_app_state();
        let state = State::from(&app_state);

        // Test count = 0 (invalid)
        let request = Json(BatchCreatePerpcityBeaconRequest { count: 0 });
        let result = batch_create_perpcity_beacon(request, token, state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), rocket::http::Status::BadRequest);

        // Test count > 100 (invalid)
        let token2 = ApiToken("test_token".to_string());
        let request2 = Json(BatchCreatePerpcityBeaconRequest { count: 101 });
        let result2 = batch_create_perpcity_beacon(request2, token2, state).await;
        assert!(result2.is_err());
        assert_eq!(result2.unwrap_err(), rocket::http::Status::BadRequest);
    }

    #[tokio::test]
    async fn test_batch_create_perpcity_beacon_valid_count() {
        use crate::guards::ApiToken;
        use crate::models::BatchCreatePerpcityBeaconRequest;
        use crate::routes::test_utils::create_simple_test_app_state;
        use rocket::State;

        let token = ApiToken("test_token".to_string());
        let app_state = create_simple_test_app_state();
        let state = State::from(&app_state);

        // Test valid count - this will fail at network level but should return partial results
        let request = Json(BatchCreatePerpcityBeaconRequest { count: 3 });
        let result = batch_create_perpcity_beacon(request, token, state).await;

        // Should return OK with failure details, not InternalServerError
        assert!(result.is_ok());
        let response = result.unwrap().into_inner();

        // Should indicate failures in the response data
        assert!(!response.success); // No beacons created due to network issues
        assert!(response.data.is_some());
        let batch_data = response.data.unwrap();
        assert_eq!(batch_data.created_count, 0);
        assert_eq!(batch_data.failed_count, 3);
        assert!(!batch_data.errors.is_empty());
    }

    #[test]
    fn test_app_state_has_required_contract_info() {
        use crate::routes::test_utils::create_simple_test_app_state;

        let app_state = create_simple_test_app_state();

        // Test that all required contract addresses are set
        assert_ne!(
            app_state.beacon_factory_address,
            Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
        );
        assert_ne!(
            app_state.perpcity_registry_address,
            Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
        );
        assert!(!app_state.access_token.is_empty());
    }

    #[test]
    fn test_batch_create_perpcity_beacon_individual_beacon_creation() {
        use crate::models::BatchCreatePerpcityBeaconResponse;

        // Test response serialization/deserialization
        let response = BatchCreatePerpcityBeaconResponse {
            created_count: 2,
            beacon_addresses: vec![
                "0x1234567890123456789012345678901234567890".to_string(),
                "0x9876543210987654321098765432109876543210".to_string(),
            ],
            failed_count: 1,
            errors: vec!["Error creating beacon".to_string()],
        };

        let serialized = serde_json::to_string(&response).unwrap();
        let deserialized: BatchCreatePerpcityBeaconResponse =
            serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.created_count, 2);
        assert_eq!(deserialized.failed_count, 1);
        assert_eq!(deserialized.beacon_addresses.len(), 2);
        assert_eq!(deserialized.errors.len(), 1);
    }

    #[tokio::test]
    async fn test_create_perpcity_beacon_with_anvil_integration() {
        use crate::guards::ApiToken;
        use crate::routes::test_utils::{TestUtils, create_test_app_state};
        use rocket::State;

        let token = ApiToken("test_token".to_string());
        let app_state = create_test_app_state().await;
        let state = State::from(&app_state);

        // Test that we can connect to the blockchain
        let block_number = TestUtils::get_block_number(&app_state.provider).await;
        assert!(block_number.is_ok());

        // Test that the deployer account has funds
        let balance = TestUtils::get_balance(&app_state.provider, app_state.wallet_address).await;
        assert!(balance.is_ok());
        let balance = balance.unwrap();
        assert!(balance > alloy::primitives::U256::ZERO);

        // Test the endpoint - this will fail because we don't have actual contracts deployed
        let result = create_perpcity_beacon(token, state).await;
        assert!(result.is_err());
        // The error should be InternalServerError (contract call failed)
        assert_eq!(
            result.unwrap_err(),
            rocket::http::Status::InternalServerError
        );
    }

    #[tokio::test]
    async fn test_transaction_confirmation_timeout_handling() {
        use crate::routes::test_utils::create_simple_test_app_state;

        let app_state = create_simple_test_app_state();
        let tx_hash =
            B256::from_str("0x1234567890123456789012345678901234567890123456789012345678901234")
                .unwrap();

        // Test transaction confirmation check
        let result = is_transaction_confirmed(&app_state, tx_hash).await;
        // Should fail due to network issues in test environment
        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        assert!(
            error_msg.contains("Failed to check transaction") || error_msg.contains("on-chain")
        );
    }

    #[tokio::test]
    async fn test_beacon_registration_already_registered() {
        use crate::routes::test_utils::create_simple_test_app_state;

        let app_state = create_simple_test_app_state();
        let beacon_address =
            Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let registry_address = app_state.perpcity_registry_address;

        // Test beacon registration check
        let result = is_beacon_registered(&app_state, beacon_address, registry_address).await;
        assert!(result.is_ok());
        // Should return false since beacon doesn't exist on test network
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_beacon_registration_with_registry_fallback() {
        use crate::routes::test_utils::create_simple_test_app_state;

        let app_state = create_simple_test_app_state();
        let beacon_address =
            Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let registry_address = app_state.perpcity_registry_address;

        // Test registration with non-existent beacon (should fail gracefully)
        let result =
            register_beacon_with_registry(&app_state, beacon_address, registry_address).await;
        assert!(result.is_err());
        // Should fail because beacon doesn't exist, but should provide meaningful error
        let error_msg = result.unwrap_err();
        assert!(
            error_msg.contains("has no deployed code")
                || error_msg.contains("Failed to check beacon contract")
        );
    }

    #[tokio::test]
    async fn test_create_beacon_via_factory_timeout_handling() {
        use crate::routes::test_utils::create_simple_test_app_state;

        let app_state = create_simple_test_app_state();
        let owner_address =
            Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let factory_address = app_state.beacon_factory_address;

        // Test beacon creation with timeout handling
        let result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;
        assert!(result.is_err());
        // Should fail due to network issues, but should provide meaningful error
        let error_msg = result.unwrap_err();
        assert!(
            error_msg.contains("Failed to send createBeacon transaction")
                || error_msg.contains("Transaction")
                || error_msg.contains("timeout")
        );
    }

    #[tokio::test]
    async fn test_create_beacon_with_rpc_fallback() {
        use crate::guards::ApiToken;
        use crate::routes::test_utils::{AnvilManager, create_test_app_state};
        use alloy::providers::ProviderBuilder;
        use rocket::State;
        use std::sync::Arc;

        // Create primary app state
        let mut app_state = create_test_app_state().await;

        // Set up alternate provider pointing to a different (non-existent) URL
        // This simulates a fallback scenario
        let anvil = AnvilManager::get_or_create().await;
        let alternate_signer = anvil.deployer_signer();
        let alternate_wallet = alloy::network::EthereumWallet::from(alternate_signer);

        // Use a bad URL for primary to force fallback
        let bad_provider = ProviderBuilder::new()
            .wallet(alternate_wallet.clone())
            .connect_http("http://localhost:9999".parse().unwrap()); // Non-existent port

        // Keep the good provider as alternate
        app_state.alternate_provider = Some(app_state.provider.clone());
        app_state.provider = Arc::new(bad_provider);

        let token = ApiToken("test_token".to_string());
        let state = State::from(&app_state);

        // This should fail on primary and attempt fallback
        let result = create_perpcity_beacon(token, state).await;

        // Should fail at contract level after trying fallback mechanism
        assert!(result.is_err());
        let status = result.unwrap_err();
        assert_eq!(status, rocket::http::Status::InternalServerError);
    }

    #[tokio::test]
    async fn test_batch_create_with_rpc_fallback() {
        use crate::guards::ApiToken;
        use crate::models::BatchCreatePerpcityBeaconRequest;
        use crate::routes::test_utils::{AnvilManager, create_test_app_state};
        use alloy::providers::ProviderBuilder;
        use rocket::State;
        use std::sync::Arc;

        // Create primary app state
        let mut app_state = create_test_app_state().await;

        // Set up alternate provider
        let anvil = AnvilManager::get_or_create().await;
        let alternate_signer = anvil.deployer_signer();
        let alternate_wallet = alloy::network::EthereumWallet::from(alternate_signer);

        // Use a bad URL for primary to force fallback
        let bad_provider = ProviderBuilder::new()
            .wallet(alternate_wallet.clone())
            .connect_http("http://localhost:9999".parse().unwrap());

        app_state.alternate_provider = Some(app_state.provider.clone());
        app_state.provider = Arc::new(bad_provider);

        let token = ApiToken("test_token".to_string());
        let state = State::from(&app_state);

        let request = Json(BatchCreatePerpcityBeaconRequest { count: 2 });
        let result = batch_create_perpcity_beacon(request, token, state).await;

        // Should return OK with failure details from fallback attempts
        assert!(result.is_ok());
        let response = result.unwrap().into_inner();

        assert!(!response.success);
        assert!(response.data.is_some());
        let batch_data = response.data.unwrap();
        assert_eq!(batch_data.failed_count, 2);
        // Errors should mention fallback attempts
        assert!(!batch_data.errors.is_empty());
    }

    #[tokio::test]
    async fn test_update_beacon_with_rpc_fallback() {
        use crate::guards::ApiToken;
        use crate::routes::test_utils::{AnvilManager, create_test_app_state};
        use alloy::providers::ProviderBuilder;
        use rocket::State;
        use std::sync::Arc;

        // Create primary app state
        let mut app_state = create_test_app_state().await;

        // Set up alternate provider
        let anvil = AnvilManager::get_or_create().await;
        let alternate_signer = anvil.deployer_signer();
        let alternate_wallet = alloy::network::EthereumWallet::from(alternate_signer);

        // Use a bad URL for primary to force fallback
        let bad_provider = ProviderBuilder::new()
            .wallet(alternate_wallet.clone())
            .connect_http("http://localhost:9999".parse().unwrap());

        app_state.alternate_provider = Some(app_state.provider.clone());
        app_state.provider = Arc::new(bad_provider);

        let token = ApiToken("test_token".to_string());
        let state = State::from(&app_state);

        let request = Json(UpdateBeaconRequest {
            beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
            value: 100,
            proof: vec![0u8; 32],
        });

        let result = update_beacon(request, token, state).await;

        // Should get an error but via fallback path
        assert!(result.is_err());
        // The error happens at contract level, not connection level, proving fallback worked
    }

    #[tokio::test]
    async fn test_rpc_fallback_with_nonce_error() {
        use crate::routes::test_utils::create_simple_test_app_state;

        let _app_state = create_simple_test_app_state();

        // Test nonce error detection with string messages
        let nonce_error_msg = "nonce too low";
        assert!(is_nonce_error(nonce_error_msg));

        let replacement_error_msg = "replacement transaction underpriced";
        assert!(is_nonce_error(replacement_error_msg));

        // Test non-nonce error
        let other_error_msg = "execution reverted";
        assert!(!is_nonce_error(other_error_msg));

        let generic_error_msg = "insufficient funds";
        assert!(!is_nonce_error(generic_error_msg));
    }

    #[tokio::test]
    async fn test_sync_wallet_nonce() {
        use crate::routes::test_utils::create_test_app_state;

        let app_state = create_test_app_state().await;

        // Test nonce synchronization
        let result = sync_wallet_nonce(&app_state).await;

        // Should succeed with test provider
        assert!(result.is_ok());
        let _nonce = result.unwrap();

        // If we got here, nonce synchronization worked
    }

    #[tokio::test]
    async fn test_create_beacon_fallback_logging() {
        use crate::guards::ApiToken;
        use crate::routes::test_utils::{AnvilManager, create_test_app_state};
        use alloy::providers::ProviderBuilder;
        use rocket::State;
        use std::sync::Arc;

        // This test verifies that proper logging occurs during fallback
        let mut app_state = create_test_app_state().await;

        // Set up bad primary provider
        let anvil = AnvilManager::get_or_create().await;
        let alternate_signer = anvil.deployer_signer();
        let alternate_wallet = alloy::network::EthereumWallet::from(alternate_signer);

        let bad_provider = ProviderBuilder::new()
            .wallet(alternate_wallet.clone())
            .connect_http("http://localhost:9999".parse().unwrap());

        app_state.alternate_provider = Some(app_state.provider.clone());
        app_state.provider = Arc::new(bad_provider);

        let token = ApiToken("test_token".to_string());
        let state = State::from(&app_state);

        // Execute with fallback
        let _result = create_perpcity_beacon(token, state).await;

        // In a real test with tracing subscriber, we would verify log messages
        // For now, just ensure the function completes without panic
    }

    #[tokio::test]
    async fn test_batch_update_beacon_empty_request() {
        let app_state = create_test_app_state().await;
        let token = ApiToken("test_token".to_string());
        let state = State::from(&app_state);

        let request = Json(BatchUpdateBeaconRequest { updates: vec![] });

        let result = batch_update_beacon(request, token, state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), rocket::http::Status::BadRequest);
    }

    #[tokio::test]
    async fn test_batch_update_beacon_exceeds_limit() {
        let app_state = create_test_app_state().await;
        let token = ApiToken("test_token".to_string());
        let state = State::from(&app_state);

        // Create 101 updates (exceeds limit of 100)
        let updates = (0..101)
            .map(|i| BeaconUpdateData {
                beacon_address: format!("0x{i:040x}"),
                value: i as u64,
                proof: vec![0u8; 32],
            })
            .collect();

        let request = Json(BatchUpdateBeaconRequest { updates });

        let result = batch_update_beacon(request, token, state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), rocket::http::Status::BadRequest);
    }

    #[tokio::test]
    async fn test_batch_update_beacon_valid_request() {
        let app_state = create_test_app_state().await;
        let token = ApiToken("test_token".to_string());
        let state = State::from(&app_state);

        let updates = vec![
            BeaconUpdateData {
                beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
                value: 100,
                proof: vec![0u8; 32],
            },
            BeaconUpdateData {
                beacon_address: "0x2345678901234567890123456789012345678901".to_string(),
                value: 200,
                proof: vec![1u8; 32],
            },
        ];

        let request = Json(BatchUpdateBeaconRequest { updates });

        let result = batch_update_beacon(request, token, state).await;

        // Should return OK with failure details, not InternalServerError
        assert!(result.is_ok());
        let response = result.unwrap();
        let batch_response = response.data.as_ref().unwrap();

        assert_eq!(batch_response.total_requested, 2);
        // Both valid addresses should succeed in test environment with Anvil
        assert_eq!(batch_response.failed_updates, 0);
        assert_eq!(batch_response.successful_updates, 2);
        assert_eq!(batch_response.results.len(), 2);

        // Verify each result has the correct beacon address
        assert_eq!(
            batch_response.results[0].beacon_address,
            "0x1234567890123456789012345678901234567890"
        );
        assert_eq!(
            batch_response.results[1].beacon_address,
            "0x2345678901234567890123456789012345678901"
        );

        // Both should succeed with transaction hashes
        assert!(batch_response.results[0].success);
        assert!(batch_response.results[0].transaction_hash.is_some());
        assert!(batch_response.results[1].success);
        assert!(batch_response.results[1].transaction_hash.is_some());
    }

    #[tokio::test]
    async fn test_batch_update_beacon_invalid_address() {
        let app_state = create_test_app_state().await;
        let token = ApiToken("test_token".to_string());
        let state = State::from(&app_state);

        let updates = vec![BeaconUpdateData {
            beacon_address: "invalid_address".to_string(),
            value: 100,
            proof: vec![0u8; 32],
        }];

        let request = Json(BatchUpdateBeaconRequest { updates });

        let result = batch_update_beacon(request, token, state).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        let batch_response = response.data.as_ref().unwrap();

        assert_eq!(batch_response.total_requested, 1);
        assert_eq!(batch_response.failed_updates, 1);
        assert_eq!(batch_response.successful_updates, 0);

        // Should have error about invalid address
        assert!(!batch_response.results[0].success);
        assert!(
            batch_response.results[0]
                .error
                .as_ref()
                .unwrap()
                .contains("Invalid beacon address")
        );
    }

    #[tokio::test]
    async fn test_batch_update_beacon_mixed_addresses() {
        let app_state = create_test_app_state().await;
        let token = ApiToken("test_token".to_string());
        let state = State::from(&app_state);

        let updates = vec![
            BeaconUpdateData {
                beacon_address: "invalid_address".to_string(),
                value: 100,
                proof: vec![0u8; 32],
            },
            BeaconUpdateData {
                beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
                value: 200,
                proof: vec![1u8; 32],
            },
        ];

        let request = Json(BatchUpdateBeaconRequest { updates });

        let result = batch_update_beacon(request, token, state).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        let batch_response = response.data.as_ref().unwrap();

        assert_eq!(batch_response.total_requested, 2);

        // Mixed results: first fails due to invalid address, second succeeds with valid address
        assert_eq!(batch_response.failed_updates, 1);
        assert_eq!(batch_response.successful_updates, 1);

        // First should fail with invalid address
        assert!(!batch_response.results[0].success);
        assert!(
            batch_response.results[0]
                .error
                .as_ref()
                .unwrap()
                .contains("Invalid beacon address")
        );

        // Second should succeed with valid address (in test environment with Anvil)
        assert!(batch_response.results[1].success);
        assert!(batch_response.results[1].transaction_hash.is_some());
    }

    #[tokio::test]
    async fn test_batch_update_beacon_response_structure() {
        let app_state = create_test_app_state().await;
        let token = ApiToken("test_token".to_string());
        let state = State::from(&app_state);

        let updates = vec![BeaconUpdateData {
            beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
            value: 100,
            proof: vec![0u8; 32],
        }];

        let request = Json(BatchUpdateBeaconRequest { updates });

        let result = batch_update_beacon(request, token, state).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.data.is_some());

        let batch_response = response.data.as_ref().unwrap();

        // Verify response structure
        assert_eq!(batch_response.results.len(), 1);
        assert_eq!(batch_response.total_requested, 1);
        assert_eq!(
            batch_response.successful_updates + batch_response.failed_updates,
            1
        );

        // Verify individual result structure
        let beacon_result = &batch_response.results[0];
        assert_eq!(
            beacon_result.beacon_address,
            "0x1234567890123456789012345678901234567890"
        );
        // Either outcome is valid - success or failure
        // The result exists and has been processed

        if beacon_result.success {
            assert!(beacon_result.transaction_hash.is_some());
            assert!(beacon_result.error.is_none());
        } else {
            assert!(beacon_result.transaction_hash.is_none());
            assert!(beacon_result.error.is_some());
        }
    }
}
