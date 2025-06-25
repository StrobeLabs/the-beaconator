use ethers::{
    contract::Contract,
    core::types::{Address, Bytes, U256},
    middleware::SignerMiddleware,
    signers::Signer,
    types::H256,
};
use rocket::serde::json::Json;
use rocket::{State, get, http::Status, post};
use std::str::FromStr;
use std::sync::Arc;
use tracing;

use crate::guards::ApiToken;
use crate::models::{
    ApiResponse, AppState, CreateBeaconRequest, DeployPerpForBeaconRequest, RegisterBeaconRequest,
    UpdateBeaconRequest,
};

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

    let signer_middleware = Arc::new(SignerMiddleware::new(
        state.provider.clone(),
        state.wallet.clone(),
    ));

    let factory_contract = Contract::new(
        factory_address,
        state.beacon_factory_abi.clone(),
        signer_middleware,
    );

    // Call createBeacon function
    let contract_call = factory_contract
        .method::<_, Address>("createBeacon", owner_address)
        .map_err(|e| format!("Failed to prepare createBeacon call: {e}"))?;

    tracing::debug!("Sending createBeacon transaction...");
    let pending_tx = contract_call
        .send()
        .await
        .map_err(|e| format!("Failed to send createBeacon transaction: {e}"))?;

    let tx_hash = pending_tx.tx_hash();
    tracing::info!("CreateBeacon transaction sent: {:#x}", tx_hash);

    // Wait for confirmation
    let receipt = pending_tx
        .await
        .map_err(|e| format!("CreateBeacon transaction failed: {e}"))?
        .ok_or("No receipt received for createBeacon transaction")?;

    tracing::info!(
        "CreateBeacon transaction confirmed: {:#x}, block: {:?}, gas used: {:?}",
        tx_hash,
        receipt.block_number,
        receipt.gas_used
    );

    // Parse the return value from logs or use eth_call to get the beacon address
    // For simplicity, we'll use the return value from the method call
    let beacon_address = contract_call
        .call()
        .await
        .map_err(|e| format!("Failed to get beacon address from createBeacon: {e}"))?;

    tracing::info!("Beacon created at address: {}", beacon_address);
    Ok(beacon_address)
}

// Helper function to register a beacon with a registry
async fn register_beacon_with_registry(
    state: &AppState,
    beacon_address: Address,
    registry_address: Address,
) -> Result<H256, String> {
    tracing::info!(
        "Registering beacon {} with registry {}",
        beacon_address,
        registry_address
    );

    let signer_middleware = Arc::new(SignerMiddleware::new(
        state.provider.clone(),
        state.wallet.clone(),
    ));

    let registry_contract = Contract::new(
        registry_address,
        state.beacon_registry_abi.clone(),
        signer_middleware,
    );

    // Call registerBeacon function
    let contract_call = registry_contract
        .method::<_, ()>("registerBeacon", beacon_address)
        .map_err(|e| format!("Failed to prepare registerBeacon call: {e}"))?;

    tracing::debug!("Sending registerBeacon transaction...");
    let pending_tx = contract_call
        .send()
        .await
        .map_err(|e| format!("Failed to send registerBeacon transaction: {e}"))?;

    let tx_hash = pending_tx.tx_hash();
    tracing::info!("RegisterBeacon transaction sent: {:#x}", tx_hash);

    // Wait for confirmation
    let receipt = pending_tx
        .await
        .map_err(|e| format!("RegisterBeacon transaction failed: {e}"))?
        .ok_or("No receipt received for registerBeacon transaction")?;

    tracing::info!(
        "RegisterBeacon transaction confirmed: {:#x}, block: {:?}, gas used: {:?}",
        tx_hash,
        receipt.block_number,
        receipt.gas_used
    );

    Ok(tx_hash)
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

    // Step 1: Deploy the beacon contract by calling createBeacon with wallet address as owner
    let owner_address = state.wallet.address();
    let beacon_address =
        match create_beacon_via_factory(state, owner_address, state.beacon_factory_address).await {
            Ok(address) => address,
            Err(e) => {
                let msg = format!("Failed to create beacon: {e}");
                tracing::error!("{}", msg);
                sentry::capture_message(&msg, sentry::Level::Error);
                return Err(Status::InternalServerError);
            }
        };

    // Step 2: Register the beacon contract with the perpcity registry
    let registration_tx_hash =
        match register_beacon_with_registry(state, beacon_address, state.perpcity_registry_address)
            .await
        {
            Ok(tx_hash) => tx_hash,
            Err(e) => {
                let msg = format!("Failed to register beacon with perpcity registry: {e}");
                tracing::error!("{}", msg);
                sentry::capture_message(&msg, sentry::Level::Error);
                return Err(Status::InternalServerError);
            }
        };

    // Step 3: Return success with beacon address
    tracing::info!(
        "Successfully created and registered perpcity beacon: {} (registration tx: {:#x})",
        beacon_address,
        registration_tx_hash
    );

    Ok(Json(ApiResponse {
        success: true,
        data: Some(format!("Beacon address: {beacon_address}")),
        message: "Perpcity beacon created and registered successfully".to_string(),
    }))
}

#[post("/deploy_perp_for_beacon", data = "<_request>")]
pub async fn deploy_perp_for_beacon(
    _request: Json<DeployPerpForBeaconRequest>,
    _token: ApiToken,
) -> Json<ApiResponse<String>> {
    tracing::info!("Received request: POST /deploy_perp_for_beacon");
    // TODO: Implement perpetual deployment for beacon
    Json(ApiResponse {
        success: false,
        data: None,
        message: "deploy_perp_for_beacon endpoint not yet implemented".to_string(),
    })
}

#[post("/update_beacon", data = "<request>")]
pub async fn update_beacon(
    request: Json<UpdateBeaconRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<String>>, Status> {
    tracing::info!(
        "Received request: POST /update_beacon for beacon {}",
        request.beacon_address
    );
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/update_beacon");
    });

    // Parse the beacon address
    let beacon_address = match Address::from_str(&request.beacon_address) {
        Ok(addr) => addr,
        Err(e) => {
            let msg = format!("Invalid beacon address format: {e}");
            tracing::error!("{}", msg);
            sentry::capture_message(&msg, sentry::Level::Error);
            return Err(Status::BadRequest);
        }
    };

    // Calculate the value: cast to uint256 and multiply by 2^96
    let value_u256 = U256::from(request.value as u128);
    let multiplier = U256::from(2u128).pow(U256::from(96u128));
    let public_signals = value_u256 * multiplier;

    // Create contract instance with signing capability
    let signer_middleware = Arc::new(SignerMiddleware::new(
        state.provider.clone(),
        state.wallet.clone(),
    ));
    let contract = Contract::new(beacon_address, state.beacon_abi.clone(), signer_middleware);

    // Prepare the updateData function call
    let proof_bytes = Bytes::from(request.proof.clone());
    let mut buf = [0u8; 32];
    public_signals.to_big_endian(&mut buf);
    let public_signals_bytes = Bytes::from(buf.to_vec());

    // Log transaction details before sending
    tracing::info!(
        "Sending transaction to beacon {} with wallet address {} on chain ID {}",
        beacon_address,
        state.wallet.address(),
        state.wallet.chain_id()
    );
    tracing::debug!(
        "Transaction details: proof_bytes length: {}, public_signals: {}",
        proof_bytes.len(),
        public_signals
    );

    // Call the updateData function using signing-enabled contract
    let contract_call = contract
        .method::<_, ()>("updateData", (proof_bytes, public_signals_bytes))
        .unwrap();

    tracing::debug!("Contract call prepared:");
    tracing::debug!("  - From address: {:?}", state.wallet.address());
    tracing::debug!("  - To contract: {:?}", beacon_address);
    tracing::debug!("  - Function: updateData");
    tracing::debug!("  - Using SignerMiddleware: true");

    match contract_call.send().await {
        Ok(tx) => {
            let tx_hash = tx.tx_hash();
            tracing::info!("Transaction sent successfully: {:#x}", tx_hash);

            // Wait for confirmation and log the result
            match tx.await {
                Ok(receipt) => {
                    if let Some(receipt) = receipt {
                        tracing::info!(
                            "Transaction confirmed: {:#x}, block: {:?}, gas used: {:?}",
                            tx_hash,
                            receipt.block_number,
                            receipt.gas_used
                        );
                        Ok(Json(ApiResponse {
                            success: true,
                            data: Some(format!("Transaction hash: {tx_hash:#x}")),
                            message: "Beacon updated successfully".to_string(),
                        }))
                    } else {
                        tracing::warn!(
                            "Transaction was sent but no receipt received: {:#x}",
                            tx_hash
                        );
                        Ok(Json(ApiResponse {
                            success: true,
                            data: Some(format!("Transaction hash: {tx_hash:#x}")),
                            message: "Transaction sent but confirmation pending".to_string(),
                        }))
                    }
                }
                Err(e) => {
                    let msg =
                        format!("Transaction failed during confirmation: {tx_hash:#x}, error: {e}");
                    tracing::error!("{}", msg);
                    sentry::capture_message(&msg, sentry::Level::Error);
                    Err(Status::InternalServerError)
                }
            }
        }
        Err(e) => {
            let error_msg = e.to_string();
            tracing::error!("Failed to send transaction: {}", error_msg);

            // Check for specific error types and return appropriate status codes
            if error_msg.contains("unknown account") {
                let msg = format!("Server account configuration error: {error_msg}");
                tracing::error!("{}", msg);
                sentry::capture_message(&msg, sentry::Level::Error);
                Err(Status::InternalServerError)
            } else if error_msg.contains("insufficient funds") {
                let msg = format!("Insufficient funds error: {error_msg}");
                tracing::error!("{}", msg);
                sentry::capture_message(&msg, sentry::Level::Error);
                Err(Status::InternalServerError)
            } else if error_msg.contains("nonce") {
                let msg = format!("Nonce error: {error_msg}");
                tracing::error!("{}", msg);
                sentry::capture_message(&msg, sentry::Level::Error);
                Err(Status::Conflict)
            } else {
                let msg = format!("Transaction error: {error_msg}");
                tracing::error!("{}", msg);
                sentry::capture_message(&msg, sentry::Level::Error);
                Err(Status::InternalServerError)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethers::{
        abi::Abi,
        core::types::Address,
        providers::{Http, Provider},
        signers::{LocalWallet, Signer},
    };
    use rocket::http::{ContentType, Status};
    use rocket::local::blocking::Client;
    use serial_test::serial;
    use std::str::FromStr;
    use std::sync::Arc;

    fn create_test_app_state() -> AppState {
        use crate::{BEACON_ABI, BEACON_FACTORY_ABI, BEACON_REGISTRY_ABI};
        use std::str::FromStr;

        let provider = Provider::<Http>::try_from("http://localhost:8545").unwrap();
        let provider = Arc::new(provider);

        // Use test private key and default to testnet for tests
        let test_private_key = "4f3edf983ac636a65a842ce7c78d9aa706d3b113b37e5a4d5edbde7e8d8be8ee";
        let chain_id = 84532u64; // Base Sepolia testnet for tests
        let wallet = test_private_key
            .parse::<LocalWallet>()
            .unwrap()
            .with_chain_id(chain_id);

        let beacon_abi: Abi = serde_json::from_str(BEACON_ABI).unwrap();
        let beacon_factory_abi: Abi = serde_json::from_str(BEACON_FACTORY_ABI).unwrap();
        let beacon_registry_abi: Abi = serde_json::from_str(BEACON_REGISTRY_ABI).unwrap();

        // Use dummy addresses for tests
        let beacon_factory_address =
            Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
        let perpcity_registry_address =
            Address::from_str("0x3456789012345678901234567890123456789012").unwrap();

        AppState {
            wallet,
            provider,
            beacon_abi,
            beacon_factory_abi,
            beacon_registry_abi,
            beacon_factory_address,
            perpcity_registry_address,
            access_token: "testtoken".to_string(),
        }
    }

    #[test]
    fn test_index() {
        let client = Client::tracked(rocket::build().mount("/", rocket::routes![index]))
            .expect("valid rocket instance");

        let response = client.get("/").dispatch();
        assert_eq!(response.status(), Status::Ok);
        assert!(response.into_string().unwrap().contains("Beaconator"));
    }

    #[test]
    fn test_all_beacons_not_implemented() {
        let app_state = create_test_app_state();
        let client = Client::tracked(
            rocket::build()
                .manage(app_state)
                .mount("/", rocket::routes![all_beacons]),
        )
        .expect("valid rocket instance");

        let response = client
            .get("/all_beacons")
            .header(rocket::http::Header::new(
                "Authorization",
                "Bearer testtoken",
            ))
            .dispatch();
        assert_eq!(response.status(), Status::Ok);
        let body = response.into_string().unwrap();
        assert!(body.contains("not yet implemented"));
    }

    #[test]
    fn test_create_beacon_not_implemented() {
        let app_state = create_test_app_state();
        let client = Client::tracked(
            rocket::build()
                .manage(app_state)
                .mount("/", rocket::routes![create_beacon]),
        )
        .expect("valid rocket instance");

        let req = CreateBeaconRequest {};
        let response = client
            .post("/create_beacon")
            .header(ContentType::JSON)
            .header(rocket::http::Header::new(
                "Authorization",
                "Bearer testtoken",
            ))
            .body(serde_json::to_string(&req).unwrap())
            .dispatch();

        assert_eq!(response.status(), Status::Ok);
        let body = response.into_string().unwrap();
        assert!(body.contains("not yet implemented"));
    }

    #[test]
    fn test_register_beacon_not_implemented() {
        let app_state = create_test_app_state();
        let client = Client::tracked(
            rocket::build()
                .manage(app_state)
                .mount("/", rocket::routes![register_beacon]),
        )
        .expect("valid rocket instance");

        let req = RegisterBeaconRequest {};
        let response = client
            .post("/register_beacon")
            .header(ContentType::JSON)
            .header(rocket::http::Header::new(
                "Authorization",
                "Bearer testtoken",
            ))
            .body(serde_json::to_string(&req).unwrap())
            .dispatch();

        assert_eq!(response.status(), Status::Ok);
        let body = response.into_string().unwrap();
        assert!(body.contains("not yet implemented"));
    }

    #[test]
    #[serial]
    fn test_create_perpcity_beacon_fails_without_network() {
        let app_state = create_test_app_state();
        let client = Client::tracked(
            rocket::build()
                .manage(app_state)
                .mount("/", rocket::routes![create_perpcity_beacon]),
        )
        .expect("valid rocket instance");

        let response = client
            .post("/create_perpcity_beacon")
            .header(rocket::http::Header::new(
                "Authorization",
                "Bearer testtoken",
            ))
            .dispatch();

        // The contract call will fail with 500 Internal Server Error because we don't have a real network
        assert_eq!(response.status(), Status::InternalServerError);
    }

    #[test]
    fn test_create_perpcity_beacon_requires_auth() {
        let app_state = create_test_app_state();
        let client = Client::tracked(
            rocket::build()
                .manage(app_state)
                .mount("/", rocket::routes![create_perpcity_beacon]),
        )
        .expect("valid rocket instance");

        let response = client
            .post("/create_perpcity_beacon")
            // No Authorization header
            .dispatch();

        // Should fail with Unauthorized due to missing auth token
        assert_eq!(response.status(), Status::Unauthorized);
    }

    #[test]
    fn test_create_perpcity_beacon_invalid_auth() {
        let app_state = create_test_app_state();
        let client = Client::tracked(
            rocket::build()
                .manage(app_state)
                .mount("/", rocket::routes![create_perpcity_beacon]),
        )
        .expect("valid rocket instance");

        let response = client
            .post("/create_perpcity_beacon")
            .header(rocket::http::Header::new(
                "Authorization",
                "Bearer invalidtoken",
            ))
            .dispatch();

        // Should fail with Unauthorized due to invalid auth token
        assert_eq!(response.status(), Status::Unauthorized);
    }

    // Unit tests for helper functions
    #[tokio::test]
    #[serial]
    async fn test_create_beacon_via_factory_helper() {
        let app_state = create_test_app_state();
        let owner_address = app_state.wallet.address();
        let factory_address = app_state.beacon_factory_address;

        // This will fail because we don't have a real network, but we can test the function exists and handles errors
        let result = create_beacon_via_factory(&app_state, owner_address, factory_address).await;

        // Should return an error since we're not connected to a real network
        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("Failed to send createBeacon transaction"));
    }

    #[tokio::test]
    #[serial]
    async fn test_register_beacon_with_registry_helper() {
        let app_state = create_test_app_state();
        let beacon_address =
            Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let registry_address = app_state.perpcity_registry_address;

        // This will fail because we don't have a real network, but we can test the function exists and handles errors
        let result =
            register_beacon_with_registry(&app_state, beacon_address, registry_address).await;

        // Should return an error since we're not connected to a real network
        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("Failed to send registerBeacon transaction"));
    }

    #[test]
    fn test_app_state_has_required_contract_info() {
        let app_state = create_test_app_state();

        // Verify all required contract addresses are set
        assert_ne!(app_state.beacon_factory_address, Address::zero());
        assert_ne!(app_state.perpcity_registry_address, Address::zero());

        // Verify all addresses are different (no duplicates)
        assert_ne!(
            app_state.beacon_factory_address,
            app_state.perpcity_registry_address
        );

        // Verify ABIs are loaded and have content
        assert!(!app_state.beacon_abi.functions.is_empty());
        assert!(!app_state.beacon_factory_abi.functions.is_empty());
        assert!(!app_state.beacon_registry_abi.functions.is_empty());

        // Verify specific functions exist in ABIs
        assert!(app_state.beacon_abi.function("updateData").is_ok());
        assert!(
            app_state
                .beacon_factory_abi
                .function("createBeacon")
                .is_ok()
        );
        assert!(
            app_state
                .beacon_registry_abi
                .function("registerBeacon")
                .is_ok()
        );
    }

    #[test]
    fn test_all_routes_exist_and_require_auth() {
        let app_state = create_test_app_state();
        let client = Client::tracked(rocket::build().manage(app_state).mount(
            "/",
            rocket::routes![
                index,
                all_beacons,
                create_beacon,
                register_beacon,
                create_perpcity_beacon,
                deploy_perp_for_beacon,
                update_beacon
            ],
        ))
        .expect("valid rocket instance");

        // Test index (no auth required)
        let response = client.get("/").dispatch();
        assert_eq!(response.status(), Status::Ok);

        // Test all authenticated routes without auth token (should fail)
        let endpoints = vec![
            "/all_beacons",
            "/create_beacon",
            "/register_beacon",
            "/create_perpcity_beacon",
            "/deploy_perp_for_beacon",
            "/update_beacon",
        ];

        for endpoint in endpoints {
            let response = if endpoint == "/all_beacons" {
                client.get(endpoint).dispatch()
            } else {
                client
                    .post(endpoint)
                    .header(ContentType::JSON)
                    .body("{}")
                    .dispatch()
            };
            assert_eq!(
                response.status(),
                Status::Unauthorized,
                "Endpoint {} should require authentication",
                endpoint
            );
        }
    }

    #[test]
    fn test_create_perpcity_beacon_with_valid_auth() {
        let app_state = create_test_app_state();
        let client = Client::tracked(
            rocket::build()
                .manage(app_state)
                .mount("/", rocket::routes![create_perpcity_beacon]),
        )
        .expect("valid rocket instance");

        // Test with valid auth token (no body required)
        let response = client
            .post("/create_perpcity_beacon")
            .header(rocket::http::Header::new(
                "Authorization",
                "Bearer testtoken",
            ))
            .dispatch();

        // Should fail with 500 due to network, not due to malformed request
        assert_eq!(response.status(), Status::InternalServerError);
    }

    #[test]
    fn test_helper_functions_exist_and_are_callable() {
        // This test ensures our helper functions exist and have the correct signatures
        // We test them indirectly through the route tests, but this validates the function signatures
        let app_state = create_test_app_state();
        let owner_address = app_state.wallet.address();
        let beacon_address =
            Address::from_str("0x1111111111111111111111111111111111111111").unwrap();

        // Test that we can call the functions (though they'll fail without network)
        let rt = tokio::runtime::Runtime::new().unwrap();

        // Test create_beacon_via_factory signature
        let create_result = rt.block_on(async {
            create_beacon_via_factory(&app_state, owner_address, app_state.beacon_factory_address)
                .await
        });
        assert!(create_result.is_err());

        // Test register_beacon_with_registry signature
        let register_result = rt.block_on(async {
            register_beacon_with_registry(
                &app_state,
                beacon_address,
                app_state.perpcity_registry_address,
            )
            .await
        });
        assert!(register_result.is_err());
    }

    #[test]
    fn test_deploy_perp_for_beacon_not_implemented() {
        let app_state = create_test_app_state();
        let client = Client::tracked(
            rocket::build()
                .manage(app_state)
                .mount("/", rocket::routes![deploy_perp_for_beacon]),
        )
        .expect("valid rocket instance");

        let req = DeployPerpForBeaconRequest {};
        let response = client
            .post("/deploy_perp_for_beacon")
            .header(ContentType::JSON)
            .header(rocket::http::Header::new(
                "Authorization",
                "Bearer testtoken",
            ))
            .body(serde_json::to_string(&req).unwrap())
            .dispatch();

        assert_eq!(response.status(), Status::Ok);
        let body = response.into_string().unwrap();
        assert!(body.contains("not yet implemented"));
    }

    #[test]
    #[serial]
    fn test_update_beacon_missing_env() {
        let app_state = create_test_app_state();
        let client = Client::tracked(
            rocket::build()
                .manage(app_state)
                .mount("/", rocket::routes![update_beacon]),
        )
        .expect("valid rocket instance");

        let req = UpdateBeaconRequest {
            beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
            value: 42,
            proof: vec![1, 2, 3],
        };
        let response = client
            .post("/update_beacon")
            .header(ContentType::JSON)
            .header(rocket::http::Header::new(
                "Authorization",
                "Bearer testtoken",
            ))
            .body(serde_json::to_string(&req).unwrap())
            .dispatch();

        // The contract call will fail with 500 Internal Server Error
        assert_eq!(response.status(), Status::InternalServerError);
    }

    #[test]
    #[serial]
    fn test_update_beacon_invalid_address() {
        let app_state = create_test_app_state();
        let client = Client::tracked(
            rocket::build()
                .manage(app_state)
                .mount("/", rocket::routes![update_beacon]),
        )
        .expect("valid rocket instance");

        let req = UpdateBeaconRequest {
            beacon_address: "not_an_address".to_string(),
            value: 42,
            proof: vec![1, 2, 3],
        };
        let response = client
            .post("/update_beacon")
            .header(ContentType::JSON)
            .header(rocket::http::Header::new(
                "Authorization",
                "Bearer testtoken",
            ))
            .body(serde_json::to_string(&req).unwrap())
            .dispatch();

        // Invalid address should return 400 Bad Request
        assert_eq!(response.status(), Status::BadRequest);
    }
}
