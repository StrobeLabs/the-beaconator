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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index() {
        let result = index();
        let response = result.into_inner();

        assert!(response.success);
        assert!(response.data.is_some());

        let api_summary = response.data.unwrap();
        assert!(api_summary.total_endpoints > 0);
        assert!(response.message.contains("Beaconator"));
        assert!(response.message.contains("endpoints"));
    }

    #[test]
    fn test_index_detailed_output() {
        let result = index();
        let response = result.into_inner();

        // Print the actual JSON output
        println!("API Response:");
        println!("{}", serde_json::to_string_pretty(&response).unwrap());

        assert!(response.success);
        assert!(response.data.is_some());

        let api_summary = response.data.unwrap();
        assert!(api_summary.total_endpoints >= 11); // We defined 11 endpoints
        assert!(api_summary.working_endpoints > 0);
        assert!(api_summary.not_implemented > 0);
        assert_eq!(api_summary.endpoints.len(), api_summary.total_endpoints);

        // Verify that the endpoints have the expected structure
        let first_endpoint = &api_summary.endpoints[0];
        assert_eq!(first_endpoint.method, "GET");
        assert_eq!(first_endpoint.path, "/");
        assert!(!first_endpoint.requires_auth);
    }

    #[test]
    fn test_all_beacons_not_implemented() {
        use crate::guards::ApiToken;

        // Create a mock ApiToken
        let token = ApiToken("test_token".to_string());

        let result = all_beacons(token);
        let response = result.into_inner();

        assert!(!response.success);
        assert!(response.message.contains("not yet implemented"));
    }
}
