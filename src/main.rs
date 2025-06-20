#[macro_use] extern crate rocket;

use ethers::{
    abi::Abi,
    contract::Contract,
    core::types::{Address, U256, Bytes},
    providers::{Http, Provider},
    signers::{LocalWallet, Signer},
};
use rocket::serde::{json::Json, Deserialize, Serialize};
use std::env;
use std::str::FromStr;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
struct UpdateBeaconRequest {
    beacon_address: String,
    value: i64,
    proof: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    message: String,
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
    "the Beaconator. A half-pound* of fresh beef, American cheese, 6 pieces of crispy Applewood smoked bacon, ketchup, and mayo. Carnivores rejoice!"
}

#[post("/update_beacon", data = "<request>")]
async fn update_beacon(request: Json<UpdateBeaconRequest>) -> Json<ApiResponse<String>> {
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/update_beacon");
    });

    dotenvy::dotenv().ok();
    let rpc_url = env::var("RPC_URL").unwrap_or_else(|_| {
        "https://mainnet.base.org".to_string()
    });
    let private_key = match env::var("PRIVATE_KEY") {
        Ok(key) => key,
        Err(_) => {
            let msg = "PRIVATE_KEY environment variable not set".to_string();
            sentry::capture_message(&msg, sentry::Level::Error);
            return Json(ApiResponse {
                success: false,
                data: None,
                message: msg,
            });
        }
    };

    // Parse the beacon address
    let beacon_address = match Address::from_str(&request.beacon_address) {
        Ok(addr) => addr,
        Err(_) => {
            let msg = "Invalid beacon address format".to_string();
            sentry::capture_message(&msg, sentry::Level::Error);
            return Json(ApiResponse {
                success: false,
                data: None,
                message: msg,
            });
        }
    };

    // Calculate the value: cast to uint256 and multiply by 2^96
    let value_u256 = U256::from(request.value as u128);
    let multiplier = U256::from(2u128).pow(U256::from(96u128));
    let public_signals = value_u256 * multiplier;

    // Create provider and wallet
    let provider = match Provider::<Http>::try_from(rpc_url) {
        Ok(p) => p,
        Err(e) => {
            let msg = format!("Failed to create provider: {e}");
            sentry::capture_message(&msg, sentry::Level::Error);
            return Json(ApiResponse {
                success: false,
                data: None,
                message: msg,
            });
        }
    };

    let wallet = match private_key.parse::<LocalWallet>() {
        Ok(w) => w.with_chain_id(8453u64), // Base mainnet chain ID
        Err(_) => {
            let msg = "Invalid private key format".to_string();
            sentry::capture_message(&msg, sentry::Level::Error);
            return Json(ApiResponse {
                success: false,
                data: None,
                message: msg,
            });
        }
    };

    // Parse the ABI
    let abi: Abi = match serde_json::from_str(BEACON_ABI) {
        Ok(abi) => abi,
        Err(_) => {
            let msg = "Failed to parse contract ABI".to_string();
            sentry::capture_message(&msg, sentry::Level::Error);
            return Json(ApiResponse {
                success: false,
                data: None,
                message: msg,
            });
        }
    };

    // Create contract instance
    let contract = Contract::new(beacon_address, abi, Arc::new(provider));

    // Prepare the updateData function call
    let proof_bytes = Bytes::from(request.proof.clone());
    let mut buf = [0u8; 32];
    public_signals.to_big_endian(&mut buf);
    let public_signals_bytes = Bytes::from(buf.to_vec());

    // Call the updateData function
    match contract
        .method::<_, ()>("updateData", (proof_bytes, public_signals_bytes))
        .unwrap()
        .from(wallet.address())
        .send()
        .await
    {
        Ok(tx) => {
            Json(ApiResponse {
                success: true,
                data: Some(format!("Transaction hash: {:?}", tx.tx_hash())),
                message: "Beacon updated successfully".to_string(),
            })
        }
        Err(e) => {
            let msg = format!("Failed to update beacon: {e}");
            sentry::capture_message(&msg, sentry::Level::Error);
            Json(ApiResponse {
                success: false,
                data: None,
                message: msg,
            })
        }
    }
}

fn rocket() -> rocket::Rocket<rocket::Build> {
    rocket::build()
        .mount("/", routes![
            index,
            update_beacon
        ])
}

#[tokio::main]
async fn main() -> Result<(), Box<rocket::Error>> {
    let dsn = std::env::var("SENTRY_DSN").ok().and_then(|s| s.parse().ok());
    let _sentry = sentry::init(sentry::ClientOptions {
        dsn,
        release: sentry::release_name!(),
        ..Default::default()
    });
    tracing_subscriber::fmt::init();
    rocket().launch().await.map(|_| ()).map_err(Box::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket::http::{ContentType, Status};
    use rocket::local::blocking::Client;
    use once_cell::sync::OnceCell;

    static INIT: OnceCell<()> = OnceCell::new();

    fn test_setup() {
        INIT.get_or_init(|| {});
    }

    #[test]
    fn test_index() {
        test_setup();
        let client = Client::tracked(rocket::build().mount("/", routes![index, update_beacon])).expect("valid rocket instance");
        let response = client.get("/").dispatch();
        assert_eq!(response.status(), Status::Ok);
        assert!(response.into_string().unwrap().contains("Beacon Update Server"));
    }

    #[test]
    #[serial]
    fn test_update_beacon_missing_env() {
        test_setup();
        // Save and clear PRIVATE_KEY
        let old = std::env::var("PRIVATE_KEY").ok();
        unsafe { std::env::remove_var("PRIVATE_KEY"); }
        let client = Client::tracked(rocket::build().mount("/", routes![index, update_beacon])).expect("valid rocket instance");
        let req = UpdateBeaconRequest {
            beacon_address: "0x1234567890123456789012345678901234567890".to_string(),
            value: 42,
            proof: vec![1, 2, 3],
        };
        let response = client.post("/update_beacon")
            .header(ContentType::JSON)
            .body(serde_json::to_string(&req).unwrap())
            .dispatch();
        assert_eq!(response.status(), Status::Ok);
        let body = response.into_string().unwrap();
        assert!(body.contains("PRIVATE_KEY environment variable not set"));
        // Restore PRIVATE_KEY
        if let Some(val) = old {
            unsafe { std::env::set_var("PRIVATE_KEY", val); }
        }
    }

    #[test]
    #[serial]
    fn test_update_beacon_invalid_address() {
        test_setup();
        // Use a truly invalid address string
        unsafe { std::env::set_var("PRIVATE_KEY", "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"); }
        let client = Client::tracked(rocket::build().mount("/", routes![index, update_beacon])).expect("valid rocket instance");
        let req = UpdateBeaconRequest {
            beacon_address: "not_an_address".to_string(),
            value: 42,
            proof: vec![1, 2, 3],
        };
        let response = client.post("/update_beacon")
            .header(ContentType::JSON)
            .body(serde_json::to_string(&req).unwrap())
            .dispatch();
        assert_eq!(response.status(), Status::Ok);
        let body = response.into_string().unwrap();
        assert!(body.contains("Invalid beacon address format"));
    }
} 