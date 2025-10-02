use rocket::get;
use rocket::serde::json::Json;
use tracing;

use crate::guards::ApiToken;
use crate::models::{ApiEndpoints, ApiResponse};

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

#[get("/all_beacons")]
pub fn all_beacons(_token: ApiToken) -> Json<ApiResponse<Vec<String>>> {
    tracing::info!("Received request: GET /all_beacons");
    // TODO: Implement beacon listing
    Json(ApiResponse {
        success: false,
        data: None,
        message: "all_beacons endpoint not yet implemented".to_string(),
    })
}

// Tests moved to tests/unit_tests/info_tests.rs
