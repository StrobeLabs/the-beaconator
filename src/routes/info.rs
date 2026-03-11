use rocket::get;
use rocket::serde::json::Json;
use rocket_okapi::openapi;
use tracing;

use crate::models::{ApiEndpoints, ApiResponse};

/// Returns API summary and available endpoints.
///
/// Provides an overview of The Beaconator API including total endpoints,
/// working endpoints, and not yet implemented endpoints. This endpoint does not require authentication.
#[openapi(tag = "Information")]
#[get("/")]
pub fn index() -> Json<ApiResponse<crate::models::ApiSummary>> {
    tracing::info!("Received request: GET /");

    let api_summary = ApiEndpoints::get_summary();
    let message = format!(
        "Welcome to The Beaconator! {} total endpoints available ({} working, {} not implemented)",
        api_summary.total_endpoints, api_summary.working_endpoints, api_summary.not_implemented
    );

    Json(ApiResponse {
        success: true,
        data: Some(api_summary),
        message,
    })
}
