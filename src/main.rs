#[macro_use] extern crate rocket;

use ethers::{
    abi::Abi,
    contract::Contract,
    core::types::{Address, U256, Bytes},
    providers::{Http, Provider},
    signers::{LocalWallet, Signer},
};
use rocket::serde::{json::Json, Deserialize, Serialize};
use rocket::{Request, request::FromRequest, request::Outcome, http::Status, State};
use std::env;
use std::str::FromStr;
use std::sync::Arc;
use tracing;

#[derive(Debug, Serialize, Deserialize)]
struct UpdateBeaconRequest {
    beacon_address: String,
    value: i64,
    proof: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateBeaconRequest {
    // TODO: Implement beacon creation parameters
}

#[derive(Debug, Serialize, Deserialize)]
struct RegisterBeaconRequest {
    // TODO: Implement beacon registration parameters
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    message: String,
}

// Cached application state
struct AppState {
    wallet: LocalWallet,
    provider: Arc<Provider<Http>>,
    beacon_abi: Abi,
    access_token: String,
}

// API Token guard
struct ApiToken;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ApiToken {
    type Error = String;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let state = request.guard::<&State<AppState>>().await;
        match state {
            Outcome::Success(state) => {
                let auth_header = request.headers().get_one("Authorization");
                match auth_header {
                    Some(header) if header.starts_with("Bearer ") => {
                        let token = &header[7..]; // Remove "Bearer " prefix
                        if token == state.access_token {
                            Outcome::Success(ApiToken)
                        } else {
                            Outcome::Error((Status::Unauthorized, "Invalid API token".to_string()))
                        }
                    }
                    _ => Outcome::Error((Status::Unauthorized, "Missing or invalid Authorization header".to_string()))
                }
            }
            _ => Outcome::Error((Status::InternalServerError, "Application state not available".to_string())),
        }
    }
}

// IBeacon interface ABI
const BEACON_ABI: &str = r#"[
    {
        "inputs": [],
        "name": "getData",
        "outputs": [
            {"name": "data", "type": "uint256"},
            {"name": "timestamp", "type": "uint256"}
        ],
        "stateMutability": "nonpayable",
        "type": "function"
    },
    {
        "inputs": [
            {"name": "proof", "type": "bytes"},
            {"name": "publicSignals", "type": "bytes"}
        ],
        "name": "updateData",
        "outputs": [],
        "stateMutability": "nonpayable",
        "type": "function"
    }
]"#;

#[get("/")]
fn index() -> &'static str {
    tracing::info!("Received request: GET /");
    "the Beaconator. A half-pound* of fresh beef, American cheese, 6 pieces of crispy Applewood smoked bacon, ketchup, and mayo. Carnivores rejoice!"
}

#[post("/create_beacon", data = "<_request>")]
async fn create_beacon(_request: Json<CreateBeaconRequest>, _token: ApiToken) -> Json<ApiResponse<String>> {
    tracing::info!("Received request: POST /create_beacon");
    // TODO: Implement beacon creation
    Json(ApiResponse {
        success: false,
        data: None,
        message: "create_beacon endpoint not yet implemented".to_string(),
    })
}

#[post("/register_beacon", data = "<_request>")]
async fn register_beacon(_request: Json<RegisterBeaconRequest>, _token: ApiToken) -> Json<ApiResponse<String>> {
    tracing::info!("Received request: POST /register_beacon");
    // TODO: Implement beacon registration
    Json(ApiResponse {
        success: false,
        data: None,
        message: "register_beacon endpoint not yet implemented".to_string(),
    })
}

#[post("/update_beacon", data = "<request>")]
async fn update_beacon(
    request: Json<UpdateBeaconRequest>, 
    _token: ApiToken,
    state: &State<AppState>
) -> Result<Json<ApiResponse<String>>, Status> {
    tracing::info!("Received request: POST /update_beacon for beacon {}", request.beacon_address);
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/update_beacon");
    });

    // Parse the beacon address
    let beacon_address = match Address::from_str(&request.beacon_address) {
        Ok(addr) => addr,
        Err(e) => {
            let msg = format!("Invalid beacon address format: {}", e);
            tracing::error!("{}", msg);
            sentry::capture_message(&msg, sentry::Level::Error);
            return Err(Status::BadRequest);
        }
    };

    // Calculate the value: cast to uint256 and multiply by 2^96
    let value_u256 = U256::from(request.value as u128);
    let multiplier = U256::from(2u128).pow(U256::from(96u128));
    let public_signals = value_u256 * multiplier;

    // Create contract instance using cached provider and ABI
    let contract = Contract::new(beacon_address, state.beacon_abi.clone(), state.provider.clone());

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

    // Call the updateData function using cached wallet
    match contract
        .method::<_, ()>("updateData", (proof_bytes, public_signals_bytes))
        .unwrap()
        .from(state.wallet.address())
        .send()
        .await
    {
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
                            data: Some(format!("Transaction hash: {:#x}", tx_hash)),
                            message: "Beacon updated successfully".to_string(),
                        }))
                    } else {
                        tracing::warn!("Transaction was sent but no receipt received: {:#x}", tx_hash);
                        Ok(Json(ApiResponse {
                            success: true,
                            data: Some(format!("Transaction hash: {:#x}", tx_hash)),
                            message: "Transaction sent but confirmation pending".to_string(),
                        }))
                    }
                }
                Err(e) => {
                    let msg = format!("Transaction failed during confirmation: {:#x}, error: {}", tx_hash, e);
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
            if error_msg.contains("unknown account") || error_msg.contains("insufficient funds") {
                let msg = format!("Account error: {}", error_msg);
                tracing::error!("{}", msg);
                sentry::capture_message(&msg, sentry::Level::Error);
                Err(Status::Unauthorized)
            } else if error_msg.contains("nonce") {
                let msg = format!("Nonce error: {}", error_msg);
                tracing::error!("{}", msg);
                sentry::capture_message(&msg, sentry::Level::Error);
                Err(Status::Conflict)
            } else {
                let msg = format!("Transaction error: {}", error_msg);
                tracing::error!("{}", msg);
                sentry::capture_message(&msg, sentry::Level::Error);
                Err(Status::InternalServerError)
            }
        }
    }
}

fn rocket() -> rocket::Rocket<rocket::Build> {
    // Load and cache environment variables
    dotenvy::dotenv().ok();
    
    let rpc_url = env::var("RPC_URL").unwrap_or_else(|_| {
        "https://mainnet.base.org".to_string()
    });
    
    let access_token = env::var("BEACONATOR_ACCESS_TOKEN").expect("BEACONATOR_ACCESS_TOKEN environment variable not set");
    
    // Parse and cache the ABI
    let beacon_abi: Abi = serde_json::from_str(BEACON_ABI).expect("Failed to parse contract ABI");
    
    // Create and cache provider
    let provider = Provider::<Http>::try_from(rpc_url.clone())
        .expect("Failed to create provider");
    let provider = Arc::new(provider);
    
    // Create and cache wallet
    let private_key = env::var("PRIVATE_KEY").expect("PRIVATE_KEY environment variable not set");
    let env_type = env::var("ENV").expect("ENV environment variable not set");
    let chain_id = match env_type.to_lowercase().as_str() {
        "testnet" => 84532u64, // Base Sepolia testnet
        "mainnet" => 8453u64,  // Base mainnet
        _ => panic!("Invalid ENV value '{}'. Must be either 'mainnet' or 'testnet'", env_type)
    };
    let wallet = private_key
        .parse::<LocalWallet>()
        .expect("Failed to parse private key")
        .with_chain_id(chain_id);
    
    let app_state = AppState {
        wallet,
        provider,
        beacon_abi,
        access_token,
    };

    rocket::build()
        .manage(app_state)
        .mount("/", routes![
            index,
            create_beacon,
            register_beacon,
            update_beacon
        ])
}

#[tokio::main]
async fn main() -> Result<(), Box<rocket::Error>> {
    tracing_subscriber::fmt::init();
    tracing::info!("Starting the Beaconator server...");
    let dsn = std::env::var("SENTRY_DSN").ok().and_then(|s| s.parse().ok());
    let _sentry = sentry::init(sentry::ClientOptions {
        dsn,
        release: sentry::release_name!(),
        ..Default::default()
    });
    let result = rocket().launch().await;
    match &result {
        Ok(_) => tracing::info!("Rocket server shut down cleanly."),
        Err(e) => tracing::error!("Rocket server failed: {e}"),
    }
    result.map(|_| ()).map_err(Box::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::http::{ContentType, Status};
    use rocket::local::blocking::Client;
    use once_cell::sync::OnceCell;
    use serial_test::serial;

    static INIT: OnceCell<()> = OnceCell::new();

    fn test_setup() {
        INIT.get_or_init(|| {});
    }

    fn test_app_state() -> AppState {
        use ethers::providers::Provider;
        use ethers::signers::LocalWallet;
        use std::sync::Arc;
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
        test_setup();
        let app_state = test_app_state();
        let client = Client::tracked(
            rocket::build()
                .manage(app_state)
                .mount("/", routes![index, create_beacon, register_beacon, update_beacon])
        ).expect("valid rocket instance");
        let response = client.get("/").dispatch();
        assert_eq!(response.status(), Status::Ok);
        assert!(response.into_string().unwrap().contains("Beaconator"));
    }

    #[test]
    fn test_create_beacon_not_implemented() {
        test_setup();
        let app_state = test_app_state();
        let client = Client::tracked(
            rocket::build()
                .manage(app_state)
                .mount("/", routes![create_beacon])
        ).expect("valid rocket instance");
        let req = CreateBeaconRequest {};
        let response = client.post("/create_beacon")
            .header(ContentType::JSON)
            .header(rocket::http::Header::new("Authorization", "Bearer testtoken"))
            .body(serde_json::to_string(&req).unwrap())
            .dispatch();
        assert_eq!(response.status(), Status::Ok);
        let body = response.into_string().unwrap();
        assert!(body.contains("not yet implemented"));
    }

    #[test]
    fn test_register_beacon_not_implemented() {
        test_setup();
        let app_state = test_app_state();
        let client = Client::tracked(
            rocket::build()
                .manage(app_state)
                .mount("/", routes![register_beacon])
        ).expect("valid rocket instance");
        let req = RegisterBeaconRequest {};
        let response = client.post("/register_beacon")
            .header(ContentType::JSON)
            .header(rocket::http::Header::new("Authorization", "Bearer testtoken"))
            .body(serde_json::to_string(&req).unwrap())
            .dispatch();
        assert_eq!(response.status(), Status::Ok);
        let body = response.into_string().unwrap();
        assert!(body.contains("not yet implemented"));
    }

    #[test]
    #[serial]
    fn test_update_beacon_missing_env() {
        test_setup();
        let app_state = test_app_state();
        let client = Client::tracked(
            rocket::build()
                .manage(app_state)
                .mount("/", routes![update_beacon])
        ).expect("valid rocket instance");
        let req = UpdateBeaconRequest {
            beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
            value: 42,
            proof: vec![1, 2, 3],
        };
        let response = client.post("/update_beacon")
            .header(ContentType::JSON)
            .header(rocket::http::Header::new("Authorization", "Bearer testtoken"))
            .body(serde_json::to_string(&req).unwrap())
            .dispatch();
        // The contract call will fail with 500 Internal Server Error
        assert_eq!(response.status(), Status::InternalServerError);
    }

    #[test]
    #[serial]
    fn test_update_beacon_invalid_address() {
        test_setup();
        let app_state = test_app_state();
        let client = Client::tracked(
            rocket::build()
                .manage(app_state)
                .mount("/", routes![update_beacon])
        ).expect("valid rocket instance");
        let req = UpdateBeaconRequest {
            beacon_address: "not_an_address".to_string(),
            value: 42,
            proof: vec![1, 2, 3],
        };
        let response = client.post("/update_beacon")
            .header(ContentType::JSON)
            .header(rocket::http::Header::new("Authorization", "Bearer testtoken"))
            .body(serde_json::to_string(&req).unwrap())
            .dispatch();
        // Invalid address should return 400 Bad Request
        assert_eq!(response.status(), Status::BadRequest);
    }
} 