use rocket::serde::json::Json;
use rocket::{State, get, http::Status};
use rocket_okapi::openapi;

use super::sentry_error;
use crate::guards::ApiToken;
use crate::models::component_factory::ComponentFactoryConfig;
use crate::models::recipe::BeaconRecipe;
use crate::models::{ApiResponse, AppState};

/// List all registered beacon recipes.
#[openapi(tag = "Recipes")]
#[get("/recipes")]
pub async fn list_recipes(
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<Vec<BeaconRecipe>>>, Status> {
    match state.registries.recipes.list_recipes().await {
        Ok(recipes) => Ok(Json(ApiResponse {
            success: true,
            data: Some(recipes),
            message: "Recipes retrieved".to_string(),
        })),
        Err(e) => {
            let error_msg = format!("Failed to list recipes: {e}");
            tracing::error!("{}", error_msg);
            sentry_error(
                &sentry::Hub::current(),
                "RegistryError",
                error_msg.clone(),
                vec![],
            );
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: error_msg,
            }))
        }
    }
}

/// Get a specific recipe by slug.
#[openapi(tag = "Recipes")]
#[get("/recipes/<slug>")]
pub async fn get_recipe(
    slug: &str,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<BeaconRecipe>>, Status> {
    match state.registries.recipes.get_recipe(slug).await {
        Ok(Some(recipe)) => Ok(Json(ApiResponse {
            success: true,
            data: Some(recipe),
            message: "Recipe retrieved".to_string(),
        })),
        Ok(None) => Ok(Json(ApiResponse {
            success: false,
            data: None,
            message: format!("Recipe '{slug}' not found"),
        })),
        Err(e) => {
            let error_msg = format!("Failed to get recipe '{slug}': {e}");
            tracing::error!("{}", error_msg);
            sentry_error(
                &sentry::Hub::current(),
                "RegistryError",
                error_msg.clone(),
                vec![],
            );
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: error_msg,
            }))
        }
    }
}

/// List all component factory addresses.
#[openapi(tag = "Factories")]
#[get("/component_factories")]
pub async fn list_component_factories(
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<Vec<ComponentFactoryConfig>>>, Status> {
    match state.registries.component_factories.list_factories().await {
        Ok(factories) => Ok(Json(ApiResponse {
            success: true,
            data: Some(factories),
            message: "Component factories retrieved".to_string(),
        })),
        Err(e) => {
            let error_msg = format!("Failed to list component factories: {e}");
            tracing::error!("{}", error_msg);
            sentry_error(
                &sentry::Hub::current(),
                "RegistryError",
                error_msg.clone(),
                vec![],
            );
            Ok(Json(ApiResponse {
                success: false,
                data: None,
                message: error_msg,
            }))
        }
    }
}
