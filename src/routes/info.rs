use rocket::get;
use rocket::serde::json::Json;
use tracing;

use crate::guards::ApiToken;
use crate::models::ApiResponse;

#[get("/")]
pub fn index() -> &'static str {
    tracing::info!("Received request: GET /");
    "Welcome to The Beaconator! Available endpoints: /create_perpcity_beacon, /batch_create_perpcity_beacon, /update_beacon, /deploy_perp_for_beacon, /batch_deploy_perps_for_beacons, /all_beacons"
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
        assert!(result.contains("Beaconator"));
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
