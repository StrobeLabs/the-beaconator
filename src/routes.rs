use ethers::{
    contract::Contract,
    core::types::{Address, Bytes, U256},
    middleware::SignerMiddleware,
    signers::Signer,
};
use rocket::serde::json::Json;
use rocket::{State, get, http::Status, post};
use std::str::FromStr;
use std::sync::Arc;
use tracing;

use crate::guards::ApiToken;
use crate::models::{
    ApiResponse, AppState, CreateBeaconRequest, RegisterBeaconRequest, UpdateBeaconRequest,
};

#[get("/")]
pub fn index() -> &'static str {
    tracing::info!("Received request: GET /");
    "the Beaconator. A half-pound* of fresh beef, American cheese, 6 pieces of crispy Applewood smoked bacon, ketchup, and mayo. Carnivores rejoice!"
}

#[post("/create_beacon", data = "<_request>")]
pub async fn create_beacon(
    _request: Json<CreateBeaconRequest>,
    _token: ApiToken,
) -> Json<ApiResponse<String>> {
    tracing::info!("Received request: POST /create_beacon");
    // TODO: Implement beacon creation
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
    use crate::BEACON_ABI;
    use ethers::{
        abi::Abi,
        providers::{Http, Provider},
        signers::{LocalWallet, Signer},
    };
    use rocket::http::{ContentType, Status};
    use rocket::local::blocking::Client;
    use serial_test::serial;
    use std::sync::Arc;

    fn create_test_app_state() -> AppState {
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
        AppState {
            wallet,
            provider,
            beacon_abi,
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
