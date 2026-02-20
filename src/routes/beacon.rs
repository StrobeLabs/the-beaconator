use alloy::primitives::{Address, B256};
use rocket::serde::json::Json;
use rocket::{State, http::Status, post};
use rocket_okapi::openapi;
use std::str::FromStr;
use tracing;

use crate::guards::ApiToken;
use crate::models::{
    ApiResponse, AppState, BatchCreateBeaconByTypeRequest, BatchCreateBeaconResponse,
    BatchUpdateBeaconRequest, BatchUpdateBeaconResponse, CreateBeaconByTypeRequest,
    CreateBeaconResponse, CreateBeaconWithEcdsaRequest, CreateBeaconWithEcdsaResponse,
    RegisterBeaconRequest, UpdateBeaconRequest, UpdateBeaconWithEcdsaRequest,
};
use crate::services::beacon::verifiable::create_verifiable_beacon_with_factory;
use crate::services::beacon::{
    batch_create_beacons as service_batch_create_beacons,
    batch_update_beacon as service_batch_update_beacon, create_and_register_beacon_by_type,
    deploy_ecdsa_verifier_adapter, register_beacon_with_registry,
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

/// Batch creates beacons using a registered beacon type.
///
/// Creates the specified number of beacons (1-100) and optionally registers them.
/// Currently only supports Simple factory types via multicall3.
#[openapi(tag = "Beacon")]
#[post("/batch_create_beacon", data = "<request>")]
pub async fn batch_create_beacon(
    request: Json<BatchCreateBeaconByTypeRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<BatchCreateBeaconResponse>>, Status> {
    tracing::info!(
        "Received request: POST /batch_create_beacon (type={}, count={})",
        request.beacon_type,
        request.count
    );
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/batch_create_beacon");
        scope.set_extra("beacon_type", request.beacon_type.clone().into());
        scope.set_extra("requested_count", request.count.into());
    });

    let count = request.count;
    if count == 0 || count > 100 {
        tracing::warn!("Invalid beacon count: {}", count);
        return Err(Status::BadRequest);
    }

    // Look up beacon type config
    let config = match state
        .beacon_type_registry
        .get_type(&request.beacon_type)
        .await
    {
        Ok(Some(config)) => config,
        Ok(None) => {
            return Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Unknown beacon type: '{}'", request.beacon_type),
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

    match service_batch_create_beacons(state.inner(), &config, count).await {
        Ok(response) => {
            let created = response.created_count;
            let failed = response.failed_count;

            let message = if failed == 0 {
                format!(
                    "Successfully created all {created} '{}' beacons",
                    config.slug
                )
            } else if created == 0 {
                "Failed to create any beacons".to_string()
            } else {
                format!("Partially successful: {created} created, {failed} failed")
            };

            tracing::info!("{}", message);

            Ok(Json(ApiResponse {
                success: created > 0,
                data: Some(response),
                message,
            }))
        }
        Err(e) => {
            tracing::error!("Batch create beacon failed: {}", e);
            Err(Status::BadRequest)
        }
    }
}

/// Creates a beacon with an auto-deployed ECDSA verifier adapter.
///
/// Deploys an ECDSAVerifierAdapter contract with the beaconator's PRIVATE_KEY signer,
/// then creates a beacon via the Dichotomous factory using the deployed verifier.
/// Optionally registers the beacon if the type has a registry configured.
#[openapi(tag = "Beacon")]
#[post("/create_beacon_with_ecdsa", data = "<request>")]
pub async fn create_beacon_with_ecdsa(
    request: Json<CreateBeaconWithEcdsaRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<CreateBeaconWithEcdsaResponse>>, Status> {
    tracing::info!(
        "Received request: POST /create_beacon_with_ecdsa (type={})",
        request.beacon_type
    );
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/create_beacon_with_ecdsa");
        scope.set_extra("beacon_type", request.beacon_type.clone().into());
        scope.set_extra("initial_data", request.initial_data.to_string().into());
        scope.set_extra("initial_cardinality", request.initial_cardinality.into());
    });

    // Look up beacon type config
    let config = match state
        .beacon_type_registry
        .get_type(&request.beacon_type)
        .await
    {
        Ok(Some(config)) => config,
        Ok(None) => {
            return Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Unknown beacon type: '{}'", request.beacon_type),
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

    // Validate factory type is Dichotomous
    if config.factory_type != crate::models::beacon_type::FactoryType::Dichotomous {
        return Ok(Json(ApiResponse {
            success: false,
            data: None,
            message: format!(
                "Beacon type '{}' uses a Simple factory; ECDSA verifier requires a Dichotomous factory",
                request.beacon_type
            ),
        }));
    }

    // Acquire wallet from pool (held for entire deploy+create flow)
    let wallet_handle = match state.wallet_manager.acquire_any_wallet().await {
        Ok(handle) => handle,
        Err(e) => {
            tracing::error!("Failed to acquire wallet: {}", e);
            sentry::capture_message(
                &format!("Failed to acquire wallet for ECDSA beacon creation: {e}"),
                sentry::Level::Error,
            );
            return Err(Status::InternalServerError);
        }
    };

    tracing::info!(
        "Acquired wallet {} for ECDSA beacon creation",
        wallet_handle.address()
    );

    // Step 1: Deploy ECDSA verifier adapter
    let verifier_address = match deploy_ecdsa_verifier_adapter(state.inner(), &wallet_handle).await
    {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("Failed to deploy ECDSA verifier adapter: {}", e);
            sentry::capture_message(
                &format!("ECDSA verifier deployment failed: {e}"),
                sentry::Level::Error,
            );
            return Err(Status::InternalServerError);
        }
    };

    tracing::info!("ECDSA verifier deployed at {}", verifier_address);

    // Step 2: Create beacon using the deployed verifier
    let beacon_address = match create_verifiable_beacon_with_factory(
        state.inner(),
        config.factory_address,
        verifier_address,
        request.initial_data,
        request.initial_cardinality,
    )
    .await
    {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("Failed to create beacon after verifier deployment: {}", e);
            sentry::capture_message(
                &format!("Beacon creation failed after ECDSA verifier deployment: {e}"),
                sentry::Level::Error,
            );
            return Err(Status::InternalServerError);
        }
    };

    // Step 3: Optionally register with registry
    let registered = if let Some(registry_address) = config.registry_address {
        match register_beacon_with_registry(state.inner(), beacon_address, registry_address).await {
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

    let response = CreateBeaconWithEcdsaResponse {
        beacon_address: format!("{beacon_address:#x}"),
        verifier_address: format!("{verifier_address:#x}"),
        beacon_type: config.slug.clone(),
        registered,
    };

    tracing::info!(
        "ECDSA beacon created: beacon={}, verifier={}, registered={}",
        response.beacon_address,
        response.verifier_address,
        registered,
    );

    sentry::capture_message(
        &format!(
            "ECDSA beacon created: {} with verifier {} (type={})",
            response.beacon_address, response.verifier_address, config.slug
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
