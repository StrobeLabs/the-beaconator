use alloy::primitives::{Address, B256};
use rocket::serde::json::Json;
use rocket::{State, http::Status, post};
use rocket_okapi::openapi;
use std::str::FromStr;
use tracing;

use crate::guards::ApiToken;
use crate::models::{
    ApiResponse, AppState, BatchCreatePerpcityBeaconRequest, BatchCreatePerpcityBeaconResponse,
    BatchUpdateBeaconRequest, BatchUpdateBeaconResponse, CreateBeaconRequest,
    CreateVerifiableBeaconRequest, RegisterBeaconRequest, UpdateBeaconRequest,
};
use crate::services::beacon::verifiable::create_verifiable_beacon as service_create_verifiable_beacon;
use crate::services::beacon::{
    batch_create_perpcity_beacon as service_batch_create_perpcity_beacon,
    batch_update_beacon as service_batch_update_beacon, create_beacon_via_factory,
    register_beacon_with_registry, update_beacon as service_update_beacon,
};

/// Creates a new beacon via the beacon factory.
///
/// Creates a beacon using the beacon factory contract for the authenticated wallet address.
#[openapi(tag = "Beacon")]
#[post("/create_beacon", data = "<_request>")]
pub async fn create_beacon(
    _request: Json<CreateBeaconRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<String>>, Status> {
    tracing::info!("Received request: POST /create_beacon");
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/create_beacon");
        scope.set_extra("wallet_address", state.wallet_address.to_string().into());
    });

    let owner_address = state.wallet_address;
    tracing::info!("Creating beacon for owner: {}", owner_address);

    match create_beacon_via_factory(state.inner(), owner_address, state.beacon_factory_address)
        .await
    {
        Ok(beacon_address) => {
            tracing::info!("Successfully created beacon at address: {}", beacon_address);
            sentry::capture_message(
                &format!("Beacon created successfully at: {beacon_address}"),
                sentry::Level::Info,
            );
            Ok(Json(ApiResponse {
                success: true,
                data: Some(beacon_address.to_string()),
                message: "Beacon created successfully".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to create beacon: {}", e);
            sentry::capture_message(
                &format!("Failed to create beacon: {e}"),
                sentry::Level::Error,
            );
            Err(Status::InternalServerError)
        }
    }
}

/// Registers an existing beacon with the registry.
///
/// Registers a previously created beacon with the PerpCity registry contract.
#[openapi(tag = "Beacon")]
#[post("/register_beacon", data = "<request>")]
pub async fn register_beacon(
    request: Json<RegisterBeaconRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<String>>, Status> {
    tracing::info!("Received request: POST /register_beacon");
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/register_beacon");
        scope.set_extra("beacon_address", request.beacon_address.clone().into());
        scope.set_extra("registry_address", request.registry_address.clone().into());
    });

    // Validate beacon address format (must start with 0x)
    if !request.beacon_address.starts_with("0x") {
        let error_msg = format!(
            "Invalid beacon address '{}': must start with 0x prefix",
            request.beacon_address
        );
        tracing::error!("{}", error_msg);
        sentry::capture_message(&error_msg, sentry::Level::Error);
        return Err(Status::BadRequest);
    }

    // Parse the beacon address
    let beacon_address = match Address::from_str(&request.beacon_address) {
        Ok(addr) => addr,
        Err(e) => {
            let error_msg = format!("Invalid beacon address '{}': {}", request.beacon_address, e);
            tracing::error!("{}", error_msg);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            return Err(Status::BadRequest);
        }
    };

    // Validate registry address format (must start with 0x)
    if !request.registry_address.starts_with("0x") {
        let error_msg = format!(
            "Invalid registry address '{}': must start with 0x prefix",
            request.registry_address
        );
        tracing::error!("{}", error_msg);
        sentry::capture_message(&error_msg, sentry::Level::Error);
        return Err(Status::BadRequest);
    }

    // Parse the registry address
    let registry_address = match Address::from_str(&request.registry_address) {
        Ok(addr) => addr,
        Err(e) => {
            let error_msg = format!(
                "Invalid registry address '{}': {}",
                request.registry_address, e
            );
            tracing::error!("{}", error_msg);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            return Err(Status::BadRequest);
        }
    };

    // Register the beacon with the specified registry
    match register_beacon_with_registry(state.inner(), beacon_address, registry_address).await {
        Ok(tx_hash) => {
            let message = if tx_hash == B256::ZERO {
                "Beacon was already registered"
            } else {
                "Beacon registered successfully"
            };
            tracing::info!(
                "{}: {} with registry {}",
                message,
                beacon_address,
                registry_address
            );
            sentry::capture_message(
                &format!("Beacon registered: {beacon_address} at registry {registry_address}"),
                sentry::Level::Info,
            );
            Ok(Json(ApiResponse {
                success: true,
                data: Some(format!("Transaction hash: {tx_hash}")),
                message: message.to_string(),
            }))
        }
        Err(e) => {
            let error_msg = format!("Failed to register beacon {beacon_address}: {e}");
            tracing::error!("{}", error_msg);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            Err(Status::InternalServerError)
        }
    }
}

/// Creates a single PerpCity beacon.
///
/// Creates a new beacon via the beacon factory and registers it with the PerpCity registry.
/// Returns the address of the created beacon on success.
#[openapi(tag = "Beacon")]
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

/// Creates multiple PerpCity beacons in a batch operation.
///
/// Creates the specified number of beacons (1-100) via the beacon factory and registers
/// them with the PerpCity registry. Returns details about successful and failed creations.
#[openapi(tag = "Beacon")]
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

/// Updates a beacon with new data using a zero-knowledge proof.
///
/// Validates the provided proof and public signals, then updates the beacon's data.
/// Returns the transaction hash on success.
#[openapi(tag = "Beacon")]
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
        scope.set_extra("public_signals_length", request.public_signals.len().into());
    });

    match service_update_beacon(state.inner(), request.into_inner()).await {
        Ok(tx_hash) => {
            tracing::info!("Successfully updated beacon. TX: {:?}", tx_hash);
            sentry::capture_message(
                &format!("Beacon updated successfully. TX: {tx_hash:?}"),
                sentry::Level::Info,
            );
            Ok(Json(ApiResponse {
                success: true,
                data: Some(format!("Transaction hash: {tx_hash:?}")),
                message: "Beacon updated successfully".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to update beacon: {}", e);
            sentry::capture_message(
                &format!("Failed to update beacon: {e}"),
                sentry::Level::Error,
            );
            Err(Status::InternalServerError)
        }
    }
}

/// Updates multiple beacons with new data using zero-knowledge proofs.
///
/// Processes a batch of beacon updates, each with their own proof and public signals.
/// Returns detailed results for each update attempt.
#[openapi(tag = "Beacon")]
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

/// Creates a verifiable beacon with Halo2 proof verification.
///
/// Creates a new verifiable beacon using the DichotomousBeaconFactory with the specified
/// verifier contract address, initial data value, and TWAP cardinality.
#[openapi(tag = "Beacon")]
#[post("/create_verifiable_beacon", data = "<request>")]
pub async fn create_verifiable_beacon(
    request: Json<CreateVerifiableBeaconRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<String>>, Status> {
    tracing::info!("Received request: POST /create_verifiable_beacon");
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/create_verifiable_beacon");
        scope.set_extra("verifier_address", request.verifier_address.clone().into());
        scope.set_extra("initial_data", request.initial_data.to_string().into());
        scope.set_extra("initial_cardinality", request.initial_cardinality.into());
    });

    match service_create_verifiable_beacon(state.inner(), request.into_inner()).await {
        Ok(beacon_address) => {
            tracing::info!(
                "Successfully created verifiable beacon at: {}",
                beacon_address
            );
            sentry::capture_message(
                &format!("Verifiable beacon created successfully at: {beacon_address}"),
                sentry::Level::Info,
            );
            Ok(Json(ApiResponse {
                success: true,
                data: Some(beacon_address),
                message: "Verifiable beacon created successfully".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to create verifiable beacon: {}", e);
            sentry::capture_message(
                &format!("Failed to create verifiable beacon: {e}"),
                sentry::Level::Error,
            );
            Err(Status::InternalServerError)
        }
    }
}
