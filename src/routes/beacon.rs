use alloy::primitives::Address;
use rocket::serde::json::Json;
use rocket::{State, http::Status, post};
use rocket_okapi::openapi;
use std::str::FromStr;
use tracing;

use crate::guards::ApiToken;
use crate::models::beacon_type::FactoryType;
use crate::models::component_factory::ComponentFactoryType;
use crate::models::recipe::{
    BaseFnSpec, BeaconKind, BeaconRecipe, PreprocessorSpec, TransformSpec,
};
use crate::models::requests::{CreateModularBeaconRequest, ModularBeaconParams};
use crate::models::responses::CreateModularBeaconResponse;
use crate::models::{
    ApiResponse, AppState, BatchUpdateBeaconRequest, BatchUpdateBeaconResponse,
    CreateBeaconByTypeRequest, CreateBeaconResponse, CreateBeaconWithEcdsaRequest,
    CreateBeaconWithEcdsaResponse, CreateLBCGBMBeaconRequest,
    CreateWeightedSumCompositeBeaconRequest, RegisterBeaconRequest, UpdateBeaconRequest,
    UpdateBeaconWithEcdsaRequest,
};
use crate::services::beacon::modular::create_modular_beacon as service_create_modular_beacon;
use crate::services::beacon::{
    RegistrationOutcome, batch_update_beacon as service_batch_update_beacon,
    create_and_register_beacon_by_type, create_and_register_factory_beacon, create_identity_beacon,
    create_weighted_sum_composite_beacon, register_beacon_with_registry,
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
        .registries
        .beacon_types
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
    let registry_address = state.contracts.perpcity_registry;
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

/// Creates an LBCGBM standalone beacon via the modular orchestrator.
///
/// Deploys a StandaloneBeacon with Identity preprocessor, CGBM base function,
/// and Bounded transform. Optionally registers with the default registry.
#[openapi(tag = "Beacon")]
#[post("/create_lbcgbm_beacon", data = "<request>")]
pub async fn create_lbcgbm_beacon_endpoint(
    request: Json<CreateLBCGBMBeaconRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<CreateBeaconResponse>>, Status> {
    tracing::info!(
        "Received request: POST /create_lbcgbm_beacon (initial_index={})",
        request.initial_index
    );
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/create_lbcgbm_beacon");
    });

    // Build modular params from the LBCGBM-specific request fields
    let modular_params = ModularBeaconParams {
        measurement_scale: Some(request.measurement_scale),
        sigma_base: Some(request.sigma_base),
        scaling_factor: Some(request.scaling_factor),
        alpha: Some(request.alpha),
        decay: Some(request.decay),
        initial_sigma_ratio: Some(request.initial_sigma_ratio),
        variance_scaling: Some(request.variance_scaling),
        min_index: Some(request.min_index),
        max_index: Some(request.max_index),
        steepness: Some(request.steepness),
        initial_index: Some(request.initial_index),
        ..Default::default()
    };

    // Build a hardcoded LBCGBM recipe
    let recipe = BeaconRecipe {
        slug: "lbcgbm".to_string(),
        name: "LBCGBM".to_string(),
        description: None,
        beacon_kind: BeaconKind::Standalone {
            preprocessor: PreprocessorSpec::Identity,
            base_fn: BaseFnSpec::CGBM,
            transform: TransformSpec::Bounded,
        },
        enabled: true,
        created_at: 0,
        updated_at: 0,
    };

    // Create the beacon via modular orchestrator
    let result = match service_create_modular_beacon(state.inner(), &recipe, &modular_params).await
    {
        Ok(result) => result,
        Err(e) => {
            let error_msg = format!("LBCGBM beacon creation failed: {e}");
            tracing::error!("{}", error_msg);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            return Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: error_msg,
            }));
        }
    };

    let beacon_address = result.beacon_address;

    // Register with perpcity registry
    let registry_address = state.contracts.perpcity_registry;
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
            let warn_msg =
                format!("LBCGBM beacon {beacon_address:#x} created but registration failed: {e}");
            tracing::warn!("{}", warn_msg);
            sentry::capture_message(&warn_msg, sentry::Level::Warning);
            (false, None)
        }
    };

    // Get the StandaloneBeaconFactory address used for LBCGBM creation
    let factory_address = state
        .registries
        .component_factories
        .get_factory_address(&ComponentFactoryType::StandaloneBeaconFactory)
        .await
        .map(|a| format!("{a:#x}"))
        .unwrap_or_else(|_| "unknown".to_string());

    let response = CreateBeaconResponse {
        beacon_address: format!("{beacon_address:#x}"),
        beacon_type: "lbcgbm".to_string(),
        factory_address,
        registered,
        safe_proposal_hash,
    };

    tracing::info!(
        "LBCGBM beacon created: beacon={}, registered={}",
        response.beacon_address,
        registered,
    );

    Ok(Json(ApiResponse {
        success: true,
        data: Some(response),
        message: "LBCGBM beacon created successfully".to_string(),
    }))
}

/// Creates a WeightedSumComposite beacon via the WeightedSumCompositeFactory.
///
/// Deploys a CompositeBeacon that computes its index as a weighted sum of
/// reference beacon indices. Optionally registers with the default registry.
#[openapi(tag = "Beacon")]
#[post("/create_weighted_sum_composite_beacon", data = "<request>")]
pub async fn create_weighted_sum_composite_beacon_endpoint(
    request: Json<CreateWeightedSumCompositeBeaconRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<CreateBeaconResponse>>, Status> {
    tracing::info!(
        "Received request: POST /create_weighted_sum_composite_beacon ({} reference beacons)",
        request.reference_beacons.len()
    );
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/create_weighted_sum_composite_beacon");
    });

    // Look up the WeightedSumComposite beacon type config from registry
    let config = match state
        .registries
        .beacon_types
        .get_type("weighted-sum-composite")
        .await
    {
        Ok(Some(config))
            if config.enabled && config.factory_type == FactoryType::WeightedSumComposite =>
        {
            config
        }
        Ok(Some(_)) => {
            let msg = "WeightedSumComposite beacon type is disabled or misconfigured";
            tracing::warn!("{}", msg);
            sentry::capture_message(msg, sentry::Level::Warning);
            return Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: msg.to_string(),
            }));
        }
        Ok(None) => {
            let msg = "WeightedSumComposite beacon type not registered. Set WEIGHTED_SUM_COMPOSITE_FACTORY_ADDRESS env var.";
            tracing::warn!("{}", msg);
            sentry::capture_message(msg, sentry::Level::Warning);
            return Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: msg.to_string(),
            }));
        }
        Err(e) => {
            let msg = format!("Failed to look up WeightedSumComposite beacon type: {e}");
            tracing::error!("{}", msg);
            sentry::capture_message(&msg, sentry::Level::Error);
            return Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: msg,
            }));
        }
    };

    // Create the beacon via factory
    let beacon_address =
        match create_weighted_sum_composite_beacon(state.inner(), &config, &request).await {
            Ok(addr) => addr,
            Err(e) => {
                let error_msg = format!("WeightedSumComposite beacon creation failed: {e}");
                tracing::error!("{}", error_msg);
                sentry::capture_message(&error_msg, sentry::Level::Error);
                return Ok(Json(ApiResponse {
                    success: false,
                    data: None,
                    message: error_msg,
                }));
            }
        };

    // Register with registry
    match create_and_register_factory_beacon(state.inner(), &config, beacon_address).await {
        Ok(response) => {
            tracing::info!(
                "WeightedSumComposite beacon created: beacon={}, registered={}",
                response.beacon_address,
                response.registered,
            );
            Ok(Json(ApiResponse {
                success: true,
                data: Some(response),
                message: "WeightedSumComposite beacon created successfully".to_string(),
            }))
        }
        Err(e) => {
            let warn_msg = format!(
                "WeightedSumComposite beacon {beacon_address:#x} created but registration failed: {e}"
            );
            tracing::warn!("{}", warn_msg);
            sentry::capture_message(&warn_msg, sentry::Level::Warning);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(CreateBeaconResponse {
                    beacon_address: format!("{beacon_address:#x}"),
                    beacon_type: config.slug.clone(),
                    factory_address: format!("{:#x}", config.factory_address),
                    registered: false,
                    safe_proposal_hash: None,
                }),
                message: warn_msg,
            }))
        }
    }
}

/// Creates a modular beacon using a named recipe.
///
/// Looks up the recipe by slug, then orchestrates multi-step creation:
/// deploying verifier, component modules, and the beacon itself via individual factory contracts.
#[openapi(tag = "Beacon")]
#[post("/create_modular_beacon", data = "<request>")]
pub async fn create_modular_beacon(
    request: Json<CreateModularBeaconRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<CreateModularBeaconResponse>>, Status> {
    tracing::info!(
        "Received request: POST /create_modular_beacon (recipe={})",
        request.recipe
    );
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/create_modular_beacon");
        scope.set_extra("recipe", request.recipe.clone().into());
    });

    // Look up recipe from registry
    let recipe = match state.registries.recipes.get_recipe(&request.recipe).await {
        Ok(Some(recipe)) => recipe,
        Ok(None) => {
            let msg = format!("Unknown recipe: '{}'", request.recipe);
            tracing::warn!("{}", msg);
            return Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: msg,
            }));
        }
        Err(e) => {
            tracing::error!("Failed to look up recipe: {}", e);
            sentry::capture_message(
                &format!("Failed to look up recipe '{}': {e}", request.recipe),
                sentry::Level::Error,
            );
            return Err(Status::InternalServerError);
        }
    };

    if !recipe.enabled {
        return Ok(Json(ApiResponse {
            success: false,
            data: None,
            message: format!("Recipe '{}' is disabled", request.recipe),
        }));
    }

    // Create the beacon via modular orchestrator
    let result = match service_create_modular_beacon(state.inner(), &recipe, &request.params).await
    {
        Ok(result) => result,
        Err(e) => {
            let error_msg = format!(
                "Modular beacon creation failed (recipe={}): {e}",
                recipe.slug
            );
            tracing::error!("{}", error_msg);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            return Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: error_msg,
            }));
        }
    };

    let beacon_address = result.beacon_address;

    // Register with perpcity registry
    let registry_address = state.contracts.perpcity_registry;
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
            let warn_msg =
                format!("Modular beacon {beacon_address:#x} created but registration failed: {e}");
            tracing::warn!("{}", warn_msg);
            sentry::capture_message(&warn_msg, sentry::Level::Warning);
            (false, None)
        }
    };

    let response = CreateModularBeaconResponse {
        beacon_address: format!("{beacon_address:#x}"),
        verifier_address: result.verifier_address.map(|a| format!("{a:#x}")),
        recipe: recipe.slug.clone(),
        components: result.components,
        registered,
        safe_proposal_hash,
    };

    tracing::info!(
        "Modular beacon created: beacon={}, recipe={}, registered={}",
        response.beacon_address,
        recipe.slug,
        registered,
    );

    sentry::capture_message(
        &format!(
            "Modular beacon created: {} (recipe={})",
            response.beacon_address, recipe.slug
        ),
        sentry::Level::Info,
    );

    Ok(Json(ApiResponse {
        success: true,
        data: Some(response),
        message: "Modular beacon created successfully".to_string(),
    }))
}
