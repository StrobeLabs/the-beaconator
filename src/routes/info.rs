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

    #[tokio::test]
    async fn test_index_detailed_output() {
        let result = index();
        let response = result.into_inner();

        assert!(response.success);
        assert!(response.data.is_some());

        let api_summary = response.data.unwrap();
        assert_eq!(api_summary.total_endpoints, api_summary.endpoints.len());
        assert!(api_summary.working_endpoints > 0);
        assert!(api_summary.not_implemented > 0);

        // Check that we have the expected endpoints
        let endpoint_paths: Vec<&str> = api_summary
            .endpoints
            .iter()
            .map(|e| e.path.as_str())
            .collect();
        assert!(endpoint_paths.contains(&"/"));
        assert!(endpoint_paths.contains(&"/all_beacons"));
        assert!(endpoint_paths.contains(&"/create_perpcity_beacon"));
        assert!(endpoint_paths.contains(&"/batch_create_perpcity_beacon"));
        assert!(endpoint_paths.contains(&"/deploy_perp_for_beacon"));
        assert!(endpoint_paths.contains(&"/deposit_liquidity_for_perp"));
        assert!(endpoint_paths.contains(&"/batch_deposit_liquidity_for_perps"));
        assert!(endpoint_paths.contains(&"/update_beacon"));
        assert!(endpoint_paths.contains(&"/fund_guest_wallet"));
    }

    #[tokio::test]
    async fn test_all_beacons_not_implemented() {
        use crate::guards::ApiToken;

        let token = ApiToken("test_token".to_string());

        let result = all_beacons(token);
        let response = result.into_inner();

        assert!(!response.success);
        assert!(response.message.contains("not yet implemented"));
    }

    #[tokio::test]
    async fn test_api_summary_serialization() {
        use crate::models::ApiSummary;

        let api_summary = ApiSummary {
            total_endpoints: 10,
            working_endpoints: 5,
            not_implemented: 3,
            deprecated: 0,
            endpoints: vec![],
        };

        // Test serialization
        let serialized = serde_json::to_string(&api_summary).unwrap();
        let deserialized: ApiSummary = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.total_endpoints, api_summary.total_endpoints);
        assert_eq!(
            deserialized.working_endpoints,
            api_summary.working_endpoints
        );
        assert_eq!(deserialized.not_implemented, api_summary.not_implemented);
        assert_eq!(deserialized.endpoints.len(), api_summary.endpoints.len());
    }

    #[tokio::test]
    async fn test_endpoint_info_structure() {
        use crate::models::{EndpointInfo, EndpointStatus};

        let endpoint = EndpointInfo {
            method: "POST".to_string(),
            path: "/test_endpoint".to_string(),
            description: "Test endpoint".to_string(),
            requires_auth: true,
            status: EndpointStatus::Working,
        };

        assert_eq!(endpoint.method, "POST");
        assert_eq!(endpoint.path, "/test_endpoint");
        assert_eq!(endpoint.description, "Test endpoint");
        assert!(endpoint.requires_auth);
        assert!(matches!(endpoint.status, EndpointStatus::Working));
    }
}
