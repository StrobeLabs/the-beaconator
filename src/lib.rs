use ethers::{
    abi::Abi,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    utils,
};
use rocket::{Build, Rocket};
use std::env;
use std::sync::Arc;

pub mod guards;
pub mod models;
pub mod routes;

use crate::models::AppState;

// IBeacon interface ABI
pub const BEACON_ABI: &str = r#"[
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

pub async fn create_rocket() -> Rocket<Build> {
    // Load and cache environment variables
    dotenvy::dotenv().ok();

    let rpc_url = env::var("RPC_URL").unwrap_or_else(|_| "https://mainnet.base.org".to_string());

    let access_token = env::var("BEACONATOR_ACCESS_TOKEN")
        .expect("BEACONATOR_ACCESS_TOKEN environment variable not set");

    // Parse and cache the ABI
    let beacon_abi: Abi = serde_json::from_str(BEACON_ABI).expect("Failed to parse contract ABI");

    // Create and cache provider
    let provider = Provider::<Http>::try_from(rpc_url.clone()).expect("Failed to create provider");
    let provider = Arc::new(provider);

    // Create and cache wallet
    let private_key = env::var("PRIVATE_KEY").expect("PRIVATE_KEY environment variable not set");
    let env_type = env::var("ENV").expect("ENV environment variable not set");

    let chain_id = match env_type.to_lowercase().as_str() {
        "testnet" => 84532u64, // Base Sepolia testnet
        "mainnet" => 8453u64,  // Base mainnet
        _ => panic!("Invalid ENV value '{env_type}'. Must be either 'mainnet' or 'testnet'"),
    };

    // Parse the wallet and log details for debugging
    let wallet = private_key
        .parse::<LocalWallet>()
        .expect("Failed to parse private key")
        .with_chain_id(chain_id);

    // Log wallet configuration for debugging
    tracing::info!("Wallet configured:");
    tracing::info!("  - Address: {:?}", wallet.address());
    tracing::info!("  - Chain ID: {}", wallet.chain_id());
    tracing::info!("  - ENV: {}", env_type);
    tracing::info!("  - RPC URL: {}", rpc_url);

    // Check wallet balance and nonce for debugging
    let wallet_address = wallet.address();
    match provider.get_balance(wallet_address, None).await {
        Ok(balance) => {
            tracing::info!("Wallet balance: {} ETH", utils::format_ether(balance));
        }
        Err(e) => {
            tracing::warn!("Failed to get wallet balance: {}", e);
        }
    }

    match provider.get_transaction_count(wallet_address, None).await {
        Ok(nonce) => {
            tracing::info!("Wallet nonce: {}", nonce);
        }
        Err(e) => {
            tracing::warn!("Failed to get wallet nonce: {}", e);
        }
    }

    let app_state = AppState {
        wallet,
        provider,
        beacon_abi,
        access_token,
    };

    rocket::build().manage(app_state).mount(
        "/",
        rocket::routes![
            routes::index,
            routes::all_beacons,
            routes::create_beacon,
            routes::deploy_perp_for_beacon,
            routes::update_beacon
        ],
    )
}
