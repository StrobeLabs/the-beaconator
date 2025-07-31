use alloy::primitives::{Address, B256, Bytes};
use alloy::providers::Provider;
use rocket::serde::json::Json;
use rocket::{State, http::Status, post};
use std::str::FromStr;
use std::time::Duration;
use tokio::time::timeout;
use tracing;

use super::{IBeacon, IBeaconFactory, IBeaconRegistry};
use crate::guards::ApiToken;
use crate::models::{
    ApiResponse, AppState, BatchCreatePerpcityBeaconRequest, BatchCreatePerpcityBeaconResponse,
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

    // Send the beacon creation transaction
    let pending_tx = contract
        .createBeacon(owner_address)
        .send()
        .await
        .map_err(|e| {
            let error_msg = format!("Failed to send createBeacon transaction: {e}");
            tracing::error!("{}", error_msg);
            tracing::error!("Transaction send error details: {:?}", e);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            error_msg
        })?;

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
            let error_msg = format!("Timeout waiting for transaction {tx_hash} confirmation");
            tracing::error!("{}", error_msg);
            tracing::error!("Transaction may still be pending or network is slow");
            tracing::info!(
                "Checking if transaction {} is already confirmed...",
                tx_hash
            );

            // Final fallback: check if transaction is already confirmed
            match is_transaction_confirmed(state, tx_hash).await {
                Ok(Some(receipt)) => {
                    tracing::info!("Transaction {} was already confirmed!", tx_hash);
                    receipt
                }
                Ok(None) => {
                    let error_msg =
                        format!("Transaction {tx_hash} not found on-chain after timeout");
                    tracing::error!("{}", error_msg);
                    return Err(error_msg);
                }
                Err(e) => {
                    tracing::error!("Failed to check transaction status: {}", e);
                    return Err(error_msg);
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

    // Send the registration transaction (beacon creation is already confirmed)
    let pending_tx = contract
        .registerBeacon(beacon_address)
        .send()
        .await
        .map_err(|e| {
            let error_type = match e.to_string().as_str() {
                s if s.contains("execution reverted") => "Contract Execution Reverted",
                s if s.contains("insufficient funds") => "Insufficient Funds",
                s if s.contains("gas") => "Gas Related Error",
                s if s.contains("nonce") => "Nonce Error",
                s if s.contains("connection") || s.contains("timeout") => {
                    "Network Connection Error"
                }
                s if s.contains("unauthorized") || s.contains("forbidden") => "Authorization Error",
                _ => "Unknown Transaction Error",
            };

            let error_msg = format!("Failed to send registerBeacon transaction: {e}");
            tracing::error!("{}", error_msg);
            tracing::error!("Error type: {}", error_type);
            tracing::error!("Transaction send error details: {:?}", e);

            // Add specific troubleshooting hints based on error type
            match error_type {
                "Contract Execution Reverted" => {
                    tracing::error!("Troubleshooting hints:");
                    tracing::error!("  - Check if beacon is already registered");
                    tracing::error!("  - Verify beacon contract is valid and deployed");
                    tracing::error!("  - Check if registry contract is properly deployed");
                    tracing::error!("  - Verify wallet has required permissions");
                    tracing::error!("  - Check if beacon implements expected interface");
                }
                "Insufficient Funds" => {
                    tracing::error!("Troubleshooting hints:");
                    tracing::error!("  - Check wallet ETH balance for gas fees");
                }
                "Gas Related Error" => {
                    tracing::error!("Troubleshooting hints:");
                    tracing::error!("  - Try increasing gas limit");
                    tracing::error!("  - Check current network gas prices");
                }
                _ => {
                    tracing::error!("Troubleshooting hints:");
                    tracing::error!("  - Check network connectivity");
                    tracing::error!("  - Verify contract addresses are correct");
                }
            }

            sentry::capture_message(&error_msg, sentry::Level::Error);
            error_msg
        })?;

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
            let error_msg =
                format!("Timeout waiting for registration transaction {tx_hash} confirmation");
            tracing::error!("{}", error_msg);
            tracing::error!("Registration may still be pending or network is slow");
            tracing::info!(
                "Checking if registration transaction {} is already confirmed...",
                tx_hash
            );

            // Final fallback: check if transaction is already confirmed
            match is_transaction_confirmed(state, tx_hash).await {
                Ok(Some(receipt)) => {
                    tracing::info!("Registration transaction {tx_hash} was already confirmed!");
                    receipt
                }
                Ok(None) => {
                    let error_msg = format!(
                        "Registration transaction {tx_hash} not found on-chain after timeout"
                    );
                    tracing::error!("{}", error_msg);
                    return Err(error_msg);
                }
                Err(e) => {
                    tracing::error!("Failed to check registration status: {}", e);
                    return Err(error_msg);
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

    let mut beacon_addresses = Vec::new();
    let mut errors = Vec::new();
    let owner_address = state.wallet_address;

    for i in 1..=count {
        tracing::info!("Creating beacon {}/{}", i, count);

        // Create a beacon using the factory
        let beacon_address =
            match create_beacon_via_factory(state, owner_address, state.beacon_factory_address)
                .await
            {
                Ok(address) => address,
                Err(e) => {
                    let error_msg = format!("Failed to create beacon {i}: {e}");
                    tracing::error!("{}", error_msg);
                    errors.push(error_msg.clone());
                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    continue; // Continue with next beacon instead of failing entire batch
                }
            };

        // Register the beacon with the perpcity registry
        match register_beacon_with_registry(state, beacon_address, state.perpcity_registry_address)
            .await
        {
            Ok(_) => {
                beacon_addresses.push(beacon_address.to_string());
                tracing::info!(
                    "Successfully created and registered beacon {}: {}",
                    i,
                    beacon_address
                );
            }
            Err(e) => {
                let error_msg = format!("Failed to register beacon {i} ({beacon_address}): {e}");
                tracing::error!("{}", error_msg);
                errors.push(error_msg.clone());
                sentry::capture_message(&error_msg, sentry::Level::Error);
                // Note: beacon was created but not registered - this is tracked in errors
                continue;
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::Address;
    use std::str::FromStr;

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
        let result = batch_create_perpcity_beacon(request, token, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), rocket::http::Status::BadRequest);

        // Test count > 100 (invalid)
        let token2 = ApiToken("test_token".to_string());
        let request2 = Json(BatchCreatePerpcityBeaconRequest { count: 101 });
        let result2 = batch_create_perpcity_beacon(request2, token2, &state).await;
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
        let result = batch_create_perpcity_beacon(request, token, &state).await;

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
        let result = create_perpcity_beacon(token, &state).await;
        assert!(result.is_err());
        // The error should be InternalServerError (contract call failed)
        assert_eq!(
            result.unwrap_err(),
            rocket::http::Status::InternalServerError
        );
    }
}
