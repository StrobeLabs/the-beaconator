use alloy::primitives::{Address, B256, Signed, U160, Uint};
use rocket::serde::json::Json;
use rocket::{State, http::Status, post};
use std::str::FromStr;
use tracing;

use super::IPerpHook;
use crate::guards::ApiToken;
use crate::models::{
    ApiResponse, AppState, BatchDeployPerpsForBeaconsRequest, BatchDeployPerpsForBeaconsResponse,
    DeployPerpForBeaconRequest,
};

// Helper function to deploy a perp for a beacon using defaults from DeployPerp.s.sol
async fn deploy_perp_for_beacon(state: &AppState, beacon_address: Address) -> Result<B256, String> {
    tracing::info!("Deploying perp for beacon {}", beacon_address);

    // Create contract instance using the sol! generated interface
    let contract = IPerpHook::new(state.perp_hook_address, &*state.provider);

    // Use defaults from DeployPerp.s.sol
    let trading_fee = Uint::<24, 1>::from(5000u32); // 0.5%
    let trading_fee_creator_split_x96 = 3951369912303465813u128; // 5% of Q96
    let min_margin = 0u128;
    let max_margin = 1_000_000_000u128; // 1000 USDC (6 decimals)
    let min_opening_leverage_x96 = 0u128;
    let max_opening_leverage_x96 = 790273926286361721684336819027u128; // 10x in Q96
    let liquidation_leverage_x96 = 790273926286361721684336819027u128; // 10x in Q96
    let liquidation_fee_x96 = 790273926286361721684336819u128; // 1% of Q96
    let liquidation_fee_split_x96 = 39513699123034658136834084095u128; // 50% of Q96
    let funding_interval = 86400i128; // 1 day in seconds
    let tick_spacing =
        Signed::<24, 1>::try_from(30i32).map_err(|e| format!("Invalid tick spacing: {e}"))?;
    let starting_sqrt_price_x96 = U160::from(560227709747861419891227623424u128); // sqrt(50) * 2^96

    // Prepare the CreatePerpParams struct with proper Alloy type constructors
    let create_perp_params = IPerpHook::CreatePerpParams {
        beacon: beacon_address,
        tradingFee: trading_fee,
        tradingFeeCreatorSplitX96: trading_fee_creator_split_x96,
        minMargin: min_margin,
        maxMargin: max_margin,
        minOpeningLeverageX96: min_opening_leverage_x96,
        maxOpeningLeverageX96: max_opening_leverage_x96,
        liquidationLeverageX96: liquidation_leverage_x96,
        liquidationFeeX96: liquidation_fee_x96,
        liquidationFeeSplitX96: liquidation_fee_split_x96,
        fundingInterval: funding_interval,
        tickSpacing: tick_spacing,
        startingSqrtPriceX96: starting_sqrt_price_x96,
    };

    tracing::debug!("Sending createPerp transaction...");

    // Send the transaction and wait for receipt
    let receipt = contract
        .createPerp(create_perp_params)
        .send()
        .await
        .map_err(|e| format!("Failed to send transaction: {e}"))?
        .get_receipt()
        .await
        .map_err(|e| format!("Failed to get receipt: {e}"))?;

    tracing::info!(
        "Perp deployment transaction confirmed with hash: {:?}",
        receipt.transaction_hash
    );

    Ok(receipt.transaction_hash)
}

#[post("/deploy_perp_for_beacon", data = "<request>")]
pub async fn deploy_perp_for_beacon_endpoint(
    request: Json<DeployPerpForBeaconRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<String>>, Status> {
    tracing::info!("Received request: POST /deploy_perp_for_beacon");
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/deploy_perp_for_beacon");
        scope.set_extra("beacon_address", request.beacon_address.clone().into());
    });

    // Parse the beacon address
    let beacon_address = match Address::from_str(&request.beacon_address) {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("Invalid beacon address: {}", e);
            return Err(Status::BadRequest);
        }
    };

    match deploy_perp_for_beacon(state, beacon_address).await {
        Ok(tx_hash) => {
            let message = "Perp deployed successfully";
            tracing::info!("{}", message);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(format!("Transaction hash: {tx_hash}")),
                message: message.to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to deploy perp: {}", e);
            sentry::capture_message(&format!("Failed to deploy perp: {e}"), sentry::Level::Error);
            Err(Status::InternalServerError)
        }
    }
}

#[post("/batch_deploy_perps_for_beacons", data = "<request>")]
pub async fn batch_deploy_perps_for_beacons(
    request: Json<BatchDeployPerpsForBeaconsRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<BatchDeployPerpsForBeaconsResponse>>, Status> {
    tracing::info!("Received request: POST /batch_deploy_perps_for_beacons");
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/batch_deploy_perps_for_beacons");
        scope.set_extra("requested_count", request.beacon_addresses.len().into());
    });

    let beacon_count = request.beacon_addresses.len();

    // Validate the count (similar to batch beacon creation)
    if beacon_count == 0 || beacon_count > 10 {
        tracing::warn!("Invalid beacon count: {}", beacon_count);
        return Err(Status::BadRequest);
    }

    let mut perp_ids = Vec::new();
    let mut errors = Vec::new();

    for (i, beacon_address) in request.beacon_addresses.iter().enumerate() {
        let index = i + 1;
        tracing::info!(
            "Deploying perp {}/{} for beacon {}",
            index,
            beacon_count,
            beacon_address
        );

        // Parse the beacon address
        let beacon_addr = match Address::from_str(beacon_address) {
            Ok(addr) => addr,
            Err(e) => {
                let error_msg =
                    format!("Failed to parse beacon address {index} ({beacon_address}): {e}");
                tracing::error!("{}", error_msg);
                errors.push(error_msg.clone());
                sentry::capture_message(&error_msg, sentry::Level::Error);
                continue;
            }
        };

        match deploy_perp_for_beacon(state, beacon_addr).await {
            Ok(tx_hash) => {
                perp_ids.push(tx_hash.to_string());
                tracing::info!(
                    "Successfully deployed perp {}: {} for beacon {}",
                    index,
                    tx_hash,
                    beacon_address
                );
            }
            Err(e) => {
                let error_msg =
                    format!("Failed to deploy perp {index} for beacon {beacon_address}: {e}");
                tracing::error!("{}", error_msg);
                errors.push(error_msg.clone());
                sentry::capture_message(&error_msg, sentry::Level::Error);
                continue; // Continue with next beacon instead of failing entire batch
            }
        }
    }

    let deployed_count = perp_ids.len() as u32;
    let failed_count = beacon_count as u32 - deployed_count;

    let response_data = BatchDeployPerpsForBeaconsResponse {
        deployed_count,
        perp_ids: perp_ids.clone(),
        failed_count,
        errors,
    };

    let message = if failed_count == 0 {
        format!("Successfully deployed perps for all {deployed_count} beacons")
    } else if deployed_count == 0 {
        "Failed to deploy any perps".to_string()
    } else {
        format!("Partially successful: {deployed_count} deployed, {failed_count} failed")
    };

    tracing::info!("{}", message);

    // Return success even with partial failures, let client handle the response
    Ok(Json(ApiResponse {
        success: deployed_count > 0,
        data: Some(response_data),
        message,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::test_utils::create_test_app_state;

    #[tokio::test]
    async fn test_deploy_perp_for_beacon_fails_without_network() {
        use crate::guards::ApiToken;
        use crate::models::DeployPerpForBeaconRequest;
        use rocket::State;

        let token = ApiToken("test_token".to_string());
        let app_state = create_test_app_state();
        let state = State::from(&app_state);

        let request = Json(DeployPerpForBeaconRequest {
            beacon_address: "0x1111111111111111111111111111111111111111".to_string(),
        });

        // This test will fail because we can't actually connect to a network
        let result = deploy_perp_for_beacon_endpoint(request, token, &state).await;
        // We expect this to fail since we don't have a real network connection
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_batch_deploy_perps_invalid_count() {
        use crate::guards::ApiToken;
        use crate::models::BatchDeployPerpsForBeaconsRequest;
        use rocket::State;

        let token = ApiToken("test_token".to_string());
        let app_state = create_test_app_state();
        let state = State::from(&app_state);

        // Test count = 0 (invalid)
        let request = Json(BatchDeployPerpsForBeaconsRequest {
            beacon_addresses: vec![],
        });
        let result = batch_deploy_perps_for_beacons(request, token, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), rocket::http::Status::BadRequest);

        // Test count > 10 (invalid)
        let token2 = ApiToken("test_token".to_string());
        let request2 = Json(BatchDeployPerpsForBeaconsRequest {
            beacon_addresses: vec!["0x1111111111111111111111111111111111111111".to_string(); 11],
        });
        let result2 = batch_deploy_perps_for_beacons(request2, token2, &state).await;
        assert!(result2.is_err());
        assert_eq!(result2.unwrap_err(), rocket::http::Status::BadRequest);
    }

    #[tokio::test]
    async fn test_batch_deploy_perps_valid_count() {
        use crate::guards::ApiToken;
        use crate::models::BatchDeployPerpsForBeaconsRequest;
        use rocket::State;

        let token = ApiToken("test_token".to_string());
        let app_state = create_test_app_state();
        let state = State::from(&app_state);

        // Test valid count - this will fail at network level but should return partial results
        let request = Json(BatchDeployPerpsForBeaconsRequest {
            beacon_addresses: vec![
                "0x1111111111111111111111111111111111111111".to_string(),
                "0x2222222222222222222222222222222222222222".to_string(),
                "0x3333333333333333333333333333333333333333".to_string(),
            ],
        });
        let result = batch_deploy_perps_for_beacons(request, token, &state).await;

        // Should return OK with failure details, not InternalServerError
        assert!(result.is_ok());
        let response = result.unwrap().into_inner();

        // Should indicate failures in the response data
        assert!(!response.success); // No perps deployed due to network issues
        assert!(response.data.is_some());
        let batch_data = response.data.unwrap();
        assert_eq!(batch_data.deployed_count, 0);
        assert_eq!(batch_data.failed_count, 3);
        assert!(!batch_data.errors.is_empty());
    }

    #[tokio::test]
    async fn test_deploy_perp_response_structure() {
        use crate::models::BatchDeployPerpsForBeaconsResponse;

        // Test response serialization/deserialization
        let response = BatchDeployPerpsForBeaconsResponse {
            deployed_count: 2,
            perp_ids: vec![
                "0x1234567890123456789012345678901234567890123456789012345678901234".to_string(),
                "0x9876543210987654321098765432109876543210987654321098765432109876".to_string(),
            ],
            failed_count: 1,
            errors: vec!["Error deploying perp".to_string()],
        };

        let serialized = serde_json::to_string(&response).unwrap();
        let deserialized: BatchDeployPerpsForBeaconsResponse =
            serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.deployed_count, 2);
        assert_eq!(deserialized.failed_count, 1);
        assert_eq!(deserialized.perp_ids.len(), 2);
        assert_eq!(deserialized.errors.len(), 1);
    }
}
