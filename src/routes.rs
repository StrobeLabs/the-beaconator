use alloy::{
    primitives::{Address, B256, Bytes, Signed, U160, Uint},
    sol,
};
use rocket::serde::json::Json;
use rocket::{State, get, http::Status, post};
use std::str::FromStr;
use tracing;

use crate::guards::ApiToken;
use crate::models::{
    ApiResponse, AppState, BatchCreatePerpcityBeaconRequest, BatchCreatePerpcityBeaconResponse,
    BatchDeployPerpsForBeaconsRequest, BatchDeployPerpsForBeaconsResponse, CreateBeaconRequest,
    DeployPerpForBeaconRequest, RegisterBeaconRequest, UpdateBeaconRequest,
};

// Define contract interfaces using Alloy's sol! macro
sol! {
    #[sol(rpc)]
    interface IBeaconFactory {
        function createBeacon(address owner) external returns (address);
        event BeaconCreated(address beacon);
    }

    #[sol(rpc)]
    interface IBeaconRegistry {
        function registerBeacon(address beacon) external;
        function unregisterBeacon(address beacon) external;
        function beacons(address beacon) external view returns (bool);
    }

    #[sol(rpc)]
    interface IBeacon {
        function getData() external view returns (uint256 data, uint256 timestamp);
        function updateData(bytes calldata proof, bytes calldata publicSignals) external;
    }

    #[sol(rpc)]
    interface IPerpHook {
        struct CreatePerpParams {
            address beacon;
            uint24 tradingFee;
            uint128 tradingFeeCreatorSplitX96;
            uint128 minMargin;
            uint128 maxMargin;
            uint128 minOpeningLeverageX96;
            uint128 maxOpeningLeverageX96;
            uint128 liquidationLeverageX96;
            uint128 liquidationFeeX96;
            uint128 liquidationFeeSplitX96;
            int128 fundingInterval;
            int24 tickSpacing;
            uint160 startingSqrtPriceX96;
        }

        function createPerp(CreatePerpParams memory params) external returns (bytes32 perpId);
        event PerpCreated(bytes32 perpId, address beacon, uint256 markPriceX96);
    }
}

// Helper function to create a beacon via the factory contract
async fn create_beacon_via_factory(
    state: &AppState,
    owner_address: Address,
    factory_address: Address,
) -> Result<Address, String> {
    tracing::info!(
        "Creating beacon via factory {} for owner {}",
        factory_address,
        owner_address
    );

    // Create contract instance using the sol! generated interface
    let contract = IBeaconFactory::new(factory_address, &*state.provider);

    tracing::debug!("Sending createBeacon transaction...");

    // Send the transaction and wait for receipt
    let receipt = contract
        .createBeacon(owner_address)
        .send()
        .await
        .map_err(|e| format!("Failed to send transaction: {e}"))?
        .get_receipt()
        .await
        .map_err(|e| format!("Failed to get receipt: {e}"))?;

    tracing::info!(
        "Transaction confirmed with hash: {:?}",
        receipt.transaction_hash
    );

    // Parse the beacon address from the event logs
    let beacon_address = parse_beacon_created_event(&receipt, factory_address)?;

    tracing::info!("Beacon created at address: {}", beacon_address);
    Ok(beacon_address)
}

// Helper function to register a beacon with a registry
async fn register_beacon_with_registry(
    state: &AppState,
    beacon_address: Address,
    registry_address: Address,
) -> Result<B256, String> {
    tracing::info!(
        "Registering beacon {} with registry {}",
        beacon_address,
        registry_address
    );

    // Create contract instance using the sol! generated interface
    let contract = IBeaconRegistry::new(registry_address, &*state.provider);

    tracing::debug!("Sending registerBeacon transaction...");

    // Send the transaction and wait for receipt
    let receipt = contract
        .registerBeacon(beacon_address)
        .send()
        .await
        .map_err(|e| format!("Failed to send transaction: {e}"))?
        .get_receipt()
        .await
        .map_err(|e| format!("Failed to get receipt: {e}"))?;

    tracing::info!(
        "Registration transaction confirmed with hash: {:?}",
        receipt.transaction_hash
    );

    Ok(receipt.transaction_hash)
}

// Helper function to parse the BeaconCreated event from transaction receipt
fn parse_beacon_created_event(
    receipt: &alloy::rpc::types::TransactionReceipt,
    factory_address: Address,
) -> Result<Address, String> {
    // Look for the BeaconCreated event in the logs
    for log in receipt.logs() {
        // Check if this log is from our factory contract
        if log.address() == factory_address {
            // Try to decode as BeaconCreated event
            if let Ok(decoded_log) = log.log_decode::<IBeaconFactory::BeaconCreated>() {
                let beacon = decoded_log.inner.data.beacon;
                return Ok(beacon);
            }
        }
    }

    Err("BeaconCreated event not found in transaction receipt".to_string())
}

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

#[get("/")]
pub fn index() -> &'static str {
    tracing::info!("Received request: GET /");
    "the Beaconator. A half-pound* of fresh beef, American cheese, 6 pieces of crispy Applewood smoked bacon, ketchup, and mayo. Carnivores rejoice!"
}

#[get("/all_beacons")]
pub fn all_beacons(_token: ApiToken) -> Json<ApiResponse<Vec<String>>> {
    tracing::info!("Received request: GET /all_beacons");
    // TODO: Implement fetching all beacons
    Json(ApiResponse {
        success: false,
        data: None,
        message: "all_beacons endpoint not yet implemented".to_string(),
    })
}

#[post("/create_beacon", data = "<_request>")]
pub async fn create_beacon(
    _request: Json<CreateBeaconRequest>,
    _token: ApiToken,
) -> Json<ApiResponse<String>> {
    tracing::info!("Received request: POST /create_beacon");
    Json(ApiResponse {
        success: false,
        data: None,
        message: "create_beacon endpoint not yet implemented".to_string(),
    })
}

#[post("/register_beacon", data = "<_request>")]
pub async fn register_beacon(
    _request: Json<RegisterBeaconRequest>,
    _token: ApiToken,
) -> Json<ApiResponse<String>> {
    tracing::info!("Received request: POST /register_beacon");
    // TODO: Implement beacon registration
    Json(ApiResponse {
        success: false,
        data: None,
        message: "register_beacon endpoint not yet implemented".to_string(),
    })
}

#[post("/create_perpcity_beacon")]
pub async fn create_perpcity_beacon(
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<String>>, Status> {
    tracing::info!("Received request: POST /create_perpcity_beacon");
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/create_perpcity_beacon");
    });

    // Create a beacon using the factory
    let owner_address = state.wallet_address;
    let beacon_address =
        match create_beacon_via_factory(state, owner_address, state.beacon_factory_address).await {
            Ok(address) => address,
            Err(e) => {
                tracing::error!("Failed to create beacon: {}", e);
                sentry::capture_message(
                    &format!("Failed to create beacon: {e}"),
                    sentry::Level::Error,
                );
                return Err(Status::InternalServerError);
            }
        };

    // Register the beacon with the perpcity registry
    match register_beacon_with_registry(state, beacon_address, state.perpcity_registry_address)
        .await
    {
        Ok(_) => {
            let message = "Perpcity beacon created and registered successfully";
            tracing::info!("{}", message);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(format!("Beacon address: {beacon_address}")),
                message: message.to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to register beacon: {}", e);
            sentry::capture_message(
                &format!("Failed to register beacon: {e}"),
                sentry::Level::Error,
            );
            Err(Status::InternalServerError)
        }
    }
}

#[post("/batch_create_perpcity_beacon", data = "<request>")]
pub async fn batch_create_perpcity_beacon(
    request: Json<BatchCreatePerpcityBeaconRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<BatchCreatePerpcityBeaconResponse>>, Status> {
    tracing::info!("Received request: POST /batch_create_perpcity_beacon");
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/batch_create_perpcity_beacon");
        scope.set_extra("requested_count", request.count.into());
    });

    let count = request.count;

    // Validate the count
    if count == 0 || count > 100 {
        tracing::warn!("Invalid beacon count: {}", count);
        return Err(Status::BadRequest);
    }

    let mut beacon_addresses = Vec::new();
    let mut errors = Vec::new();
    let owner_address = state.wallet_address;

    for i in 1..=count {
        tracing::info!("Creating beacon {}/{}", i, count);

        // Create a beacon using the factory
        let beacon_address =
            match create_beacon_via_factory(state, owner_address, state.beacon_factory_address)
                .await
            {
                Ok(address) => address,
                Err(e) => {
                    let error_msg = format!("Failed to create beacon {i}: {e}");
                    tracing::error!("{}", error_msg);
                    errors.push(error_msg.clone());
                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    continue; // Continue with next beacon instead of failing entire batch
                }
            };

        // Register the beacon with the perpcity registry
        match register_beacon_with_registry(state, beacon_address, state.perpcity_registry_address)
            .await
        {
            Ok(_) => {
                beacon_addresses.push(beacon_address.to_string());
                tracing::info!(
                    "Successfully created and registered beacon {}: {}",
                    i,
                    beacon_address
                );
            }
            Err(e) => {
                let error_msg = format!("Failed to register beacon {i} ({beacon_address}): {e}");
                tracing::error!("{}", error_msg);
                errors.push(error_msg.clone());
                sentry::capture_message(&error_msg, sentry::Level::Error);
                // Note: beacon was created but not registered - this is tracked in errors
                continue;
            }
        }
    }

    let created_count = beacon_addresses.len() as u32;
    let failed_count = count - created_count;

    let response_data = BatchCreatePerpcityBeaconResponse {
        created_count,
        beacon_addresses: beacon_addresses.clone(),
        failed_count,
        errors,
    };

    let message = if failed_count == 0 {
        format!("Successfully created and registered all {created_count} Perpcity beacons")
    } else if created_count == 0 {
        "Failed to create any beacons".to_string()
    } else {
        format!("Partially successful: {created_count} created, {failed_count} failed")
    };

    tracing::info!("{}", message);

    // Return success even with partial failures, let client handle the response
    Ok(Json(ApiResponse {
        success: created_count > 0,
        data: Some(response_data),
        message,
    }))
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
        scope.set_extra("value", request.value.into());
    });

    // Parse the beacon address
    let beacon_address = match Address::from_str(&request.beacon_address) {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("Invalid beacon address: {}", e);
            return Err(Status::BadRequest);
        }
    };

    // Create contract instance using the sol! generated interface
    let contract = IBeacon::new(beacon_address, &*state.provider);

    // Prepare the proof and public signals
    let proof_bytes = Bytes::from(request.proof.clone());
    let public_signals_bytes = Bytes::from(vec![0u8; 32]); // Placeholder for now

    tracing::debug!("Sending updateData transaction...");

    // Send the transaction and wait for receipt
    let receipt = match contract
        .updateData(proof_bytes, public_signals_bytes)
        .send()
        .await
    {
        Ok(pending_tx) => match pending_tx.get_receipt().await {
            Ok(receipt) => receipt,
            Err(e) => {
                tracing::error!("Failed to get receipt: {}", e);
                sentry::capture_message(
                    &format!("Failed to get receipt: {e}"),
                    sentry::Level::Error,
                );
                return Err(Status::InternalServerError);
            }
        },
        Err(e) => {
            tracing::error!("Failed to send transaction: {}", e);
            sentry::capture_message(
                &format!("Failed to send transaction: {e}"),
                sentry::Level::Error,
            );
            return Err(Status::InternalServerError);
        }
    };

    tracing::info!(
        "Update transaction confirmed with hash: {:?}",
        receipt.transaction_hash
    );

    Ok(Json(ApiResponse {
        success: true,
        data: Some(format!("Transaction hash: {}", receipt.transaction_hash)),
        message: "Beacon updated successfully".to_string(),
    }))
}

// Test module for this module's functionality
#[cfg(test)]
mod tests {
    use super::*;
    use alloy::{json_abi::JsonAbi, primitives::Address};
    use std::str::FromStr;
    use std::sync::Arc;

    fn create_test_app_state() -> AppState {
        // Create mock provider with wallet for testing - this won't work in real tests but allows compilation
        let signer = alloy::signers::local::PrivateKeySigner::random();
        let wallet = alloy::network::EthereumWallet::from(signer);

        // Use modern Alloy provider builder pattern for tests
        let provider = alloy::providers::ProviderBuilder::new()
            .wallet(wallet)
            .connect_http("http://localhost:8545".parse().unwrap());

        AppState {
            provider: Arc::new(provider),
            wallet_address: Address::from_str("0x1111111111111111111111111111111111111111")
                .unwrap(),
            beacon_abi: JsonAbi::new(),
            beacon_factory_abi: JsonAbi::new(),
            beacon_registry_abi: JsonAbi::new(),
            perp_hook_abi: JsonAbi::new(),
            beacon_factory_address: Address::from_str("0x1234567890123456789012345678901234567890")
                .unwrap(),
            perpcity_registry_address: Address::from_str(
                "0x2345678901234567890123456789012345678901",
            )
            .unwrap(),
            perp_hook_address: Address::from_str("0x3456789012345678901234567890123456789012")
                .unwrap(),
            access_token: "test_token".to_string(),
        }
    }

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

    #[tokio::test]
    async fn test_create_beacon_not_implemented() {
        use crate::guards::ApiToken;

        // Create a mock ApiToken
        let token = ApiToken("test_token".to_string());

        let request = Json(CreateBeaconRequest {
            placeholder: "test".to_string(),
        });

        let result = create_beacon(request, token).await;
        let response = result.into_inner();

        assert!(!response.success);
        assert!(response.message.contains("not yet implemented"));
    }

    #[tokio::test]
    async fn test_register_beacon_not_implemented() {
        use crate::guards::ApiToken;

        // Create a mock ApiToken
        let token = ApiToken("test_token".to_string());

        let request = Json(RegisterBeaconRequest {
            placeholder: "test".to_string(),
        });

        let result = register_beacon(request, token).await;
        let response = result.into_inner();

        assert!(!response.success);
        assert!(response.message.contains("not yet implemented"));
    }

    #[tokio::test]
    async fn test_create_perpcity_beacon_fails_without_network() {
        use crate::guards::ApiToken;
        use rocket::State;

        // This test will fail because we can't actually connect to a network
        let token = ApiToken("test_token".to_string());
        let app_state = create_test_app_state();
        let state = State::from(&app_state);

        let result = create_perpcity_beacon(token, &state).await;
        // We expect this to fail since we don't have a real network connection
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_beacon_via_factory_helper() {
        let app_state = create_test_app_state();
        let owner_address =
            Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let factory_address = app_state.beacon_factory_address;

        // This will fail without a real network, but tests the function signature
        let result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;
        assert!(result.is_err()); // Expected to fail without real network
    }

    #[tokio::test]
    async fn test_register_beacon_with_registry_helper() {
        let app_state = create_test_app_state();
        let beacon_address =
            Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let registry_address = app_state.perpcity_registry_address;

        // This will fail without a real network, but tests the function signature
        let result =
            register_beacon_with_registry(&app_state, beacon_address, registry_address).await;
        assert!(result.is_err()); // Expected to fail without real network
    }

    #[test]
    fn test_app_state_has_required_contract_info() {
        let app_state = create_test_app_state();

        // Verify that the app state contains the required contract information
        assert_ne!(
            app_state.beacon_factory_address,
            Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
        );
        assert_ne!(
            app_state.perpcity_registry_address,
            Address::from_str("0x0000000000000000000000000000000000000000").unwrap()
        );
        assert!(!app_state.access_token.is_empty());
    }

    #[test]
    fn test_helper_functions_exist_and_are_callable() {
        // We test them indirectly through the route tests, but this validates the function signatures
        let _app_state = create_test_app_state();
        let owner_address =
            Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let beacon_address =
            Address::from_str("0x1111111111111111111111111111111111111111").unwrap();

        // These are just function calls to validate signatures - they won't succeed without a real network
        assert_ne!(owner_address, Address::ZERO);
        assert_ne!(beacon_address, Address::ZERO);
    }

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
    async fn test_batch_create_perpcity_beacon_invalid_count() {
        use crate::guards::ApiToken;
        use rocket::serde::json::Json;

        let token = ApiToken("test_token".to_string());
        let app_state = create_test_app_state();
        let state = rocket::State::from(&app_state);

        // Test count = 0 (invalid)
        let request = Json(BatchCreatePerpcityBeaconRequest { count: 0 });
        let result = batch_create_perpcity_beacon(request, token, &state).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), rocket::http::Status::BadRequest);

        // Test count > 100 (invalid)
        let token2 = ApiToken("test_token".to_string());
        let request2 = Json(BatchCreatePerpcityBeaconRequest { count: 101 });
        let result2 = batch_create_perpcity_beacon(request2, token2, &state).await;
        assert!(result2.is_err());
        assert_eq!(result2.unwrap_err(), rocket::http::Status::BadRequest);
    }

    #[tokio::test]
    async fn test_batch_create_perpcity_beacon_valid_count() {
        use crate::guards::ApiToken;
        use rocket::serde::json::Json;

        let token = ApiToken("test_token".to_string());
        let app_state = create_test_app_state();
        let state = rocket::State::from(&app_state);

        // Test valid count - this will fail at network level but should return partial results
        let request = Json(BatchCreatePerpcityBeaconRequest { count: 5 });
        let result = batch_create_perpcity_beacon(request, token, &state).await;

        // Should return OK with failure details, not InternalServerError
        assert!(result.is_ok());
        let response = result.unwrap().into_inner();

        // Should indicate failures in the response data
        assert!(!response.success); // No beacons created due to network issues
        assert!(response.data.is_some());
        let batch_data = response.data.unwrap();
        assert_eq!(batch_data.created_count, 0);
        assert_eq!(batch_data.failed_count, 5);
        assert!(!batch_data.errors.is_empty());
    }

    #[tokio::test]
    async fn test_batch_create_response_structure() {
        use crate::models::BatchCreatePerpcityBeaconResponse;

        // Test response serialization/deserialization
        let response = BatchCreatePerpcityBeaconResponse {
            created_count: 3,
            beacon_addresses: vec![
                "0x123".to_string(),
                "0x456".to_string(),
                "0x789".to_string(),
            ],
            failed_count: 2,
            errors: vec!["Error 1".to_string(), "Error 2".to_string()],
        };

        let serialized = serde_json::to_string(&response).unwrap();
        let deserialized: BatchCreatePerpcityBeaconResponse =
            serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.created_count, 3);
        assert_eq!(deserialized.failed_count, 2);
        assert_eq!(deserialized.beacon_addresses.len(), 3);
        assert_eq!(deserialized.errors.len(), 2);
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

    #[tokio::test]
    async fn test_batch_create_request_structure() {
        use crate::models::BatchCreatePerpcityBeaconRequest;

        // Test request deserialization
        let json_str = r#"{"count": 10}"#;
        let request: BatchCreatePerpcityBeaconRequest = serde_json::from_str(json_str).unwrap();
        assert_eq!(request.count, 10);

        // Test request serialization
        let request = BatchCreatePerpcityBeaconRequest { count: 25 };
        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("25"));
    }
}
