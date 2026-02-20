use alloy::primitives::Address;
use rocket::serde::json::Json;
use rocket::{State, delete, get, http::Status, post, put};
use rocket_okapi::openapi;
use std::str::FromStr;

use crate::guards::AdminToken;
use crate::models::{
    ApiResponse, AppState, BeaconTypeConfig, BeaconTypeListResponse, RegisterBeaconTypeRequest,
    UpdateBeaconTypeRequest,
};

/// List all registered beacon types.
#[openapi(tag = "Beacon Types (Admin)")]
#[get("/beacon_types")]
pub async fn list_beacon_types(
    _token: AdminToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<BeaconTypeListResponse>>, Status> {
    match state.beacon_type_registry.list_types().await {
        Ok(beacon_types) => Ok(Json(ApiResponse {
            success: true,
            data: Some(BeaconTypeListResponse { beacon_types }),
            message: "Beacon types retrieved".to_string(),
        })),
        Err(e) => {
            tracing::error!("Failed to list beacon types: {}", e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Failed to list beacon types: {e}"),
            }))
        }
    }
}

/// Get a specific beacon type by slug.
#[openapi(tag = "Beacon Types (Admin)")]
#[get("/beacon_type/<slug>")]
pub async fn get_beacon_type(
    slug: &str,
    _token: AdminToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<BeaconTypeConfig>>, Status> {
    match state.beacon_type_registry.get_type(slug).await {
        Ok(Some(config)) => Ok(Json(ApiResponse {
            success: true,
            data: Some(config),
            message: "Beacon type retrieved".to_string(),
        })),
        Ok(None) => Ok(Json(ApiResponse {
            success: false,
            data: None,
            message: format!("Beacon type '{slug}' not found"),
        })),
        Err(e) => {
            tracing::error!("Failed to get beacon type '{}': {}", slug, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Failed to get beacon type: {e}"),
            }))
        }
    }
}

/// Register a new beacon type.
#[openapi(tag = "Beacon Types (Admin)")]
#[post("/beacon_types", data = "<request>")]
pub async fn register_beacon_type(
    request: Json<RegisterBeaconTypeRequest>,
    _token: AdminToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<BeaconTypeConfig>>, Status> {
    let factory_address = match Address::from_str(&request.factory_address) {
        Ok(addr) => addr,
        Err(e) => {
            return Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Invalid factory_address: {e}"),
            }));
        }
    };

    let registry_address = match &request.registry_address {
        Some(addr_str) => match Address::from_str(addr_str) {
            Ok(addr) => Some(addr),
            Err(e) => {
                return Ok(Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!("Invalid registry_address: {e}"),
                }));
            }
        },
        None => None,
    };

    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let config = BeaconTypeConfig {
        slug: request.slug.clone(),
        name: request.name.clone(),
        description: request.description.clone(),
        factory_address,
        factory_type: request.factory_type.clone(),
        registry_address,
        enabled: request.enabled.unwrap_or(true),
        created_at: now_ts,
        updated_at: now_ts,
    };

    match state.beacon_type_registry.register_type(&config).await {
        Ok(()) => {
            tracing::info!("Registered beacon type '{}'", config.slug);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(config),
                message: "Beacon type registered".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to register beacon type: {}", e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Failed to register beacon type: {e}"),
            }))
        }
    }
}

/// Update an existing beacon type.
#[openapi(tag = "Beacon Types (Admin)")]
#[put("/beacon_type/<slug>", data = "<request>")]
pub async fn update_beacon_type(
    slug: &str,
    request: Json<UpdateBeaconTypeRequest>,
    _token: AdminToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<BeaconTypeConfig>>, Status> {
    // Get existing config
    let existing = match state.beacon_type_registry.get_type(slug).await {
        Ok(Some(config)) => config,
        Ok(None) => {
            return Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Beacon type '{slug}' not found"),
            }));
        }
        Err(e) => {
            return Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Failed to get beacon type: {e}"),
            }));
        }
    };

    // Merge updates
    let factory_address = match &request.factory_address {
        Some(addr_str) => match Address::from_str(addr_str) {
            Ok(addr) => addr,
            Err(e) => {
                return Ok(Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!("Invalid factory_address: {e}"),
                }));
            }
        },
        None => existing.factory_address,
    };

    let registry_address = match &request.registry_address {
        Some(addr_str) => match Address::from_str(addr_str) {
            Ok(addr) => Some(addr),
            Err(e) => {
                return Ok(Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!("Invalid registry_address: {e}"),
                }));
            }
        },
        None => existing.registry_address,
    };

    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let updated = BeaconTypeConfig {
        slug: existing.slug,
        name: request.name.clone().unwrap_or(existing.name),
        description: request.description.clone().or(existing.description),
        factory_address,
        factory_type: request
            .factory_type
            .clone()
            .unwrap_or(existing.factory_type),
        registry_address,
        enabled: request.enabled.unwrap_or(existing.enabled),
        created_at: existing.created_at,
        updated_at: now_ts,
    };

    match state.beacon_type_registry.update_type(slug, &updated).await {
        Ok(()) => {
            tracing::info!("Updated beacon type '{}'", slug);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(updated),
                message: "Beacon type updated".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to update beacon type '{}': {}", slug, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Failed to update beacon type: {e}"),
            }))
        }
    }
}

/// Delete a beacon type.
#[openapi(tag = "Beacon Types (Admin)")]
#[delete("/beacon_type/<slug>")]
pub async fn delete_beacon_type(
    slug: &str,
    _token: AdminToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<String>>, Status> {
    match state.beacon_type_registry.delete_type(slug).await {
        Ok(()) => {
            tracing::info!("Deleted beacon type '{}'", slug);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(slug.to_string()),
                message: "Beacon type deleted".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to delete beacon type '{}': {}", slug, e);
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: format!("Failed to delete beacon type: {e}"),
            }))
        }
    }
}
