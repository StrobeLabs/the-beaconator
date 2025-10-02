use alloy::primitives::{Address, B256, Bytes};
use rocket::serde::json::Json;
use rocket::{State, http::Status, post};
use std::str::FromStr;
use tracing;

use super::IBeacon;
use crate::guards::ApiToken;
use crate::models::{
    ApiResponse, AppState, BatchCreatePerpcityBeaconRequest, BatchCreatePerpcityBeaconResponse,
    BatchUpdateBeaconRequest, BatchUpdateBeaconResponse, CreateBeaconRequest,
    RegisterBeaconRequest, UpdateBeaconRequest,
};
use crate::services::beacon::{
    batch_create_perpcity_beacon as service_batch_create_perpcity_beacon,
    batch_update_beacon as service_batch_update_beacon, create_beacon_via_factory,
    register_beacon_with_registry,
};
use crate::services::transaction::events::parse_data_updated_event;

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
    let owner_address = state.wallet_address;

    // Validate the count
    if count == 0 || count > 100 {
        tracing::warn!("Invalid beacon count: {}", count);
        return Err(Status::BadRequest);
    }

    // Use the extracted service function
    match service_batch_create_perpcity_beacon(state.inner(), count, owner_address).await {
        Ok(response_data) => {
            let created_count = response_data.created_count;
            let failed_count = response_data.failed_count;

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
        Err(error) => {
            tracing::error!("Batch create perpcity beacon failed: {}", error);
            Err(Status::BadRequest)
        }
    }
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
        scope.set_extra("proof_length", request.proof.len().into());
        scope.set_extra("signals_length", request.public_signals.len().into());
    });

    // Parse the beacon address
    let beacon_address = match Address::from_str(&request.beacon_address) {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("Invalid beacon address: {}", e);
            return Err(Status::BadRequest);
        }
    };

    // Parse proof and public signals from hex strings
    let proof_bytes = match hex::decode(request.proof.trim_start_matches("0x")) {
        Ok(bytes) => Bytes::from(bytes),
        Err(e) => {
            tracing::error!("Invalid proof hex: {}", e);
            return Err(Status::BadRequest);
        }
    };

    let public_signals_bytes = match hex::decode(request.public_signals.trim_start_matches("0x")) {
        Ok(bytes) => Bytes::from(bytes),
        Err(e) => {
            tracing::error!("Invalid public signals hex: {}", e);
            return Err(Status::BadRequest);
        }
    };

    // Create contract instance using the sol! generated interface
    let contract = IBeacon::new(beacon_address, &*state.provider);

    tracing::debug!(
        "Sending updateData transaction with proof ({} bytes) and signals ({} bytes)...",
        proof_bytes.len(),
        public_signals_bytes.len()
    );

    // Send the transaction and wait for receipt
    let receipt = match contract
        .updateData(proof_bytes.clone(), public_signals_bytes.clone())
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
        "Update transaction confirmed in block {:?}",
        receipt.block_number
    );

    // Parse the DataUpdated event to confirm the beacon was actually updated
    let updated_data = match parse_data_updated_event(&receipt, beacon_address) {
        Ok(data) => data,
        Err(e) => {
            tracing::error!("Failed to parse DataUpdated event: {}", e);
            sentry::capture_message(&e, sentry::Level::Error);
            return Err(Status::InternalServerError);
        }
    };

    tracing::info!(
        "Beacon updated successfully with new data: {}",
        updated_data
    );

    let message = "Beacon updated successfully";
    Ok(Json(ApiResponse {
        success: true,
        data: Some(format!(
            "Transaction hash: {:?}, Updated data: {}",
            receipt.transaction_hash, updated_data
        )),
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

    // Use the extracted service function
    match service_batch_update_beacon(state.inner(), &request.updates).await {
        Ok(response) => {
            let message = format!(
                "Batch update completed: {}/{} successful",
                response.successful_updates, response.total_requested
            );

            Ok(Json(ApiResponse {
                success: response.successful_updates > 0,
                data: Some(response),
                message,
            }))
        }
        Err(error) => {
            tracing::error!("Batch update beacon failed: {}", error);
            Err(Status::BadRequest)
        }
    }
}

// Tests moved to tests/unit_tests/beacon_tests.rs
