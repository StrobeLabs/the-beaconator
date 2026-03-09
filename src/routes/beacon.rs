use alloy::primitives::Address;
use rocket::serde::json::Json;
use rocket::{State, http::Status, post};
use rocket_okapi::openapi;
use std::str::FromStr;
use tracing;

use crate::guards::ApiToken;
use crate::models::{
    ApiResponse, AppState, BatchUpdateBeaconRequest, BatchUpdateBeaconResponse,
    CreateBeaconByTypeRequest, CreateBeaconResponse, CreateBeaconWithEcdsaRequest,
    CreateBeaconWithEcdsaResponse, RegisterBeaconRequest, UpdateBeaconRequest,
    UpdateBeaconWithEcdsaRequest,
};
use crate::services::beacon::{
    RegistrationOutcome, batch_update_beacon as service_batch_update_beacon,
    create_and_register_beacon_by_type, create_identity_beacon, register_beacon_with_registry,
    update_beacon as service_update_beacon,
    update_beacon_with_ecdsa as service_update_beacon_with_ecdsa,
};

/// Creates a new beacon using a registered beacon type.
///
/// Looks up the beacon type by slug from the registry, then dispatches creation
/// to the correct factory. Optionally registers the beacon if the type has a registry configured.
#[openapi(tag = "Beacon")]
#[post("/create_beacon", data = "<request>")]
pub async fn create_beacon(
    request: Json<CreateBeaconByTypeRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<CreateBeaconResponse>>, Status> {
    tracing::info!(
        "Received request: POST /create_beacon (type={})",
        request.beacon_type
    );
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/create_beacon");
        scope.set_extra("beacon_type", request.beacon_type.clone().into());
    });

    // Look up beacon type config from registry
    let config = match state
        .beacon_type_registry
        .get_type(&request.beacon_type)
        .await
    {
        Ok(Some(config)) => config,
        Ok(None) => {
            let msg = format!("Unknown beacon type: '{}'", request.beacon_type);
            tracing::warn!("{}", msg);
            return Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: msg,
            }));
        }
        Err(e) => {
            tracing::error!("Failed to look up beacon type: {}", e);
            return Err(Status::InternalServerError);
        }
    };

    if !config.enabled {
        return Ok(Json(ApiResponse {
            success: false,
            data: None,
            message: format!("Beacon type '{}' is disabled", request.beacon_type),
        }));
    }

    match create_and_register_beacon_by_type(state.inner(), &config, request.params.as_ref()).await
    {
        Ok(response) => {
            tracing::info!(
                "Created '{}' beacon at {}",
                config.slug,
                response.beacon_address
            );
            sentry::capture_message(
                &format!(
                    "Beacon created: {} (type={})",
                    response.beacon_address, config.slug
                ),
                sentry::Level::Info,
            );
            Ok(Json(ApiResponse {
                success: true,
                data: Some(response),
                message: "Beacon created successfully".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to create '{}' beacon: {}", config.slug, e);
            sentry::capture_message(
                &format!("Failed to create beacon (type={}): {e}", config.slug),
                sentry::Level::Error,
            );
            Err(Status::InternalServerError)
        }
    }
}

/// Creates an IdentityBeacon with an auto-deployed ECDSA verifier.
///
/// Creates an ECDSAVerifier via the factory contract with the beaconator's PRIVATE_KEY signer,
/// then deploys an IdentityBeacon using the verifier. Optionally registers with the default registry.
#[openapi(tag = "Beacon")]
#[post("/create_beacon_with_ecdsa", data = "<request>")]
pub async fn create_beacon_with_ecdsa(
    request: Json<CreateBeaconWithEcdsaRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<CreateBeaconWithEcdsaResponse>>, Status> {
    tracing::info!(
        "Received request: POST /create_beacon_with_ecdsa (initial_index={})",
        request.initial_index
    );
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/create_beacon_with_ecdsa");
        scope.set_extra("initial_index", request.initial_index.to_string().into());
    });

    // Create IdentityBeacon with ECDSA verifier (handles verifier creation + beacon deployment)
    let (beacon_address, verifier_address) =
        match create_identity_beacon(state.inner(), request.initial_index).await {
            Ok(result) => result,
            Err(e) => {
                let error_msg = format!("ECDSA beacon creation failed: {e}");
                tracing::error!("{}", error_msg);
                sentry::capture_message(&error_msg, sentry::Level::Error);
                return Ok(Json(ApiResponse {
                    success: false,
                    data: None,
                    message: error_msg,
                }));
            }
        };

    // Register with the perpcity registry
    let registry_address = state.perpcity_registry_address;
    let (registered, safe_proposal_hash) = match register_beacon_with_registry(
        state.inner(),
        beacon_address,
        registry_address,
    )
    .await
    {
        Ok(RegistrationOutcome::OnChainConfirmed(_))
        | Ok(RegistrationOutcome::AlreadyRegistered) => {
            tracing::info!(
                "Beacon {} registered with registry {}",
                beacon_address,
                registry_address
            );
            (true, None)
        }
        Ok(RegistrationOutcome::SafeProposed(hash)) => {
            tracing::info!(
                "Beacon {} Safe registration proposed (hash: {}), not yet confirmed",
                beacon_address,
                hash
            );
            (false, Some(format!("{hash:#x}")))
        }
        Err(e) => {
            let warn_msg = format!("Beacon {beacon_address} created but registration failed: {e}");
            tracing::warn!("{}", warn_msg);
            sentry::capture_message(&warn_msg, sentry::Level::Warning);
            (false, None)
        }
    };

    let response = CreateBeaconWithEcdsaResponse {
        beacon_address: format!("{beacon_address:#x}"),
        verifier_address: format!("{verifier_address:#x}"),
        beacon_type: "identity".to_string(),
        registered,
        safe_proposal_hash,
    };

    tracing::info!(
        "ECDSA beacon created: beacon={}, verifier={}, registered={}",
        response.beacon_address,
        response.verifier_address,
        registered,
    );

    sentry::capture_message(
        &format!(
            "ECDSA beacon created: {} with verifier {}",
            response.beacon_address, response.verifier_address
        ),
        sentry::Level::Info,
    );

    Ok(Json(ApiResponse {
        success: true,
        data: Some(response),
        message: "Beacon created with ECDSA verifier successfully".to_string(),
    }))
}

/// Registers an existing beacon with a registry contract.
///
/// Registers a previously created beacon with the specified registry contract.
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
        Ok(outcome) => {
            let (message, data) = match &outcome {
                RegistrationOutcome::AlreadyRegistered => (
                    "Beacon was already registered",
                    "Already registered".to_string(),
                ),
                RegistrationOutcome::SafeProposed(hash) => (
                    "Safe transaction proposed for beacon registration",
                    format!("Safe tx hash: {hash}"),
                ),
                RegistrationOutcome::OnChainConfirmed(hash) => (
                    "Beacon registered successfully",
                    format!("Transaction hash: {hash}"),
                ),
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
                data: Some(data),
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

/// Updates a beacon using ECDSA signature from the beaconator wallet.
///
/// This endpoint is for beacons that use an ECDSAVerifierAdapter for verification.
/// The beaconator wallet signs the measurement value and submits it to the beacon.
/// The beacon's verifier must have the beaconator wallet configured as the designated signer.
#[openapi(tag = "Beacon")]
#[post("/update_beacon_with_ecdsa_adapter", data = "<request>")]
pub async fn update_beacon_with_ecdsa_adapter(
    request: Json<UpdateBeaconWithEcdsaRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<String>>, Status> {
    tracing::info!("Received request: POST /update_beacon_with_ecdsa_adapter");
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/update_beacon_with_ecdsa_adapter");
        scope.set_extra("beacon_address", request.beacon_address.clone().into());
        scope.set_extra("measurement", request.measurement.clone().into());
    });

    match service_update_beacon_with_ecdsa(state.inner(), request.into_inner()).await {
        Ok(tx_hash) => {
            tracing::info!(
                "Successfully updated beacon with ECDSA signature. TX: {:?}",
                tx_hash
            );
            sentry::capture_message(
                &format!("Beacon updated with ECDSA signature. TX: {tx_hash:?}"),
                sentry::Level::Info,
            );
            Ok(Json(ApiResponse {
                success: true,
                data: Some(format!("Transaction hash: {tx_hash:?}")),
                message: "Beacon updated successfully with ECDSA signature".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to update beacon with ECDSA signature: {}", e);
            sentry::capture_message(
                &format!("Failed to update beacon with ECDSA signature: {e}"),
                sentry::Level::Error,
            );
            Err(Status::InternalServerError)
        }
    }
}
