use alloy::{
    json_abi::JsonAbi,
    network::EthereumWallet,
    primitives::{Address, utils::format_ether},
    providers::{Provider, ProviderBuilder, WalletProvider},
    signers::{Signer, local::PrivateKeySigner},
};
use rocket::{Build, Rocket};
use std::env;
use std::str::FromStr;
use std::sync::Arc;

pub mod guards;
pub mod models;
pub mod routes;

use crate::models::AppState;

// Let Rust infer the complex provider type
pub type AlloyProvider = alloy::providers::fillers::FillProvider<
    alloy::providers::fillers::JoinFill<
        alloy::providers::fillers::JoinFill<
            alloy::providers::Identity,
            alloy::providers::fillers::JoinFill<
                alloy::providers::fillers::GasFiller,
                alloy::providers::fillers::JoinFill<
                    alloy::providers::fillers::BlobGasFiller,
                    alloy::providers::fillers::JoinFill<
                        alloy::providers::fillers::NonceFiller,
                        alloy::providers::fillers::ChainIdFiller,
                    >,
                >,
            >,
        >,
        alloy::providers::fillers::WalletFiller<alloy::network::EthereumWallet>,
    >,
    alloy::providers::RootProvider<alloy::network::Ethereum>,
    alloy::network::Ethereum,
>;

// Load ABIs from files
fn load_abi(name: &str) -> JsonAbi {
    let abi_path = format!("abis/{name}.json");
    let abi_content = std::fs::read_to_string(&abi_path)
        .unwrap_or_else(|_| panic!("Failed to read ABI file: {abi_path}"));
    serde_json::from_str(&abi_content)
        .unwrap_or_else(|_| panic!("Failed to parse ABI file: {abi_path}"))
}

pub async fn create_rocket() -> Rocket<Build> {
    // Load and cache environment variables
    dotenvy::dotenv().ok();

    let rpc_url = env::var("RPC_URL").unwrap_or_else(|_| "https://mainnet.base.org".to_string());

    let access_token = env::var("BEACONATOR_ACCESS_TOKEN")
        .expect("BEACONATOR_ACCESS_TOKEN environment variable not set");

    // Load ABIs from files
    let beacon_abi = load_abi("Beacon");
    let beacon_factory_abi = load_abi("BeaconFactory");
    let beacon_registry_abi = load_abi("BeaconRegistry");
    let perp_hook_abi = load_abi("PerpHook");

    // Load contract addresses
    let beacon_factory_address = Address::from_str(
        &env::var("BEACON_FACTORY_ADDRESS")
            .expect("BEACON_FACTORY_ADDRESS environment variable not set"),
    )
    .expect("Failed to parse beacon factory address");

    let perpcity_registry_address = Address::from_str(
        &env::var("PERPCITY_REGISTRY_ADDRESS")
            .expect("PERPCITY_REGISTRY_ADDRESS environment variable not set"),
    )
    .expect("Failed to parse perpcity registry address");

    let perp_hook_address = Address::from_str(
        &env::var("PERP_HOOK_ADDRESS").expect("PERP_HOOK_ADDRESS environment variable not set"),
    )
    .expect("Failed to parse perp hook address");

    // Get environment configuration
    let env_type = env::var("ENV").expect("ENV environment variable not set");

    let chain_id = match env_type.to_lowercase().as_str() {
        "testnet" => 84532u64,  // Base Sepolia testnet
        "mainnet" => 8453u64,   // Base mainnet
        "localnet" => 84532u64, // Use testnet chain ID for local development/CI
        _ => panic!(
            "Invalid ENV value '{env_type}'. Must be either 'mainnet', 'testnet', or 'localnet'"
        ),
    };

    // Parse the wallet and create EthereumWallet
    let private_key = env::var("PRIVATE_KEY").expect("PRIVATE_KEY environment variable not set");
    let signer = private_key
        .parse::<PrivateKeySigner>()
        .expect("Failed to parse private key")
        .with_chain_id(Some(chain_id));

    let wallet = EthereumWallet::from(signer);

    // Create provider with wallet using modern Alloy patterns
    let provider_impl = ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(rpc_url.parse().expect("Invalid RPC URL"));

    // Log wallet configuration for debugging
    let wallet_address = provider_impl.default_signer_address();
    tracing::info!("Wallet configured:");
    tracing::info!("  - Address: {:?}", wallet_address);
    tracing::info!("  - Chain ID: {:?}", chain_id);
    tracing::info!("  - ENV: {}", env_type);
    tracing::info!("  - RPC URL: {}", rpc_url);

    // Check wallet balance and nonce for debugging
    match provider_impl.get_balance(wallet_address).await {
        Ok(balance) => {
            tracing::info!("Wallet balance: {} ETH", format_ether(balance));
        }
        Err(e) => {
            tracing::warn!("Failed to get wallet balance: {}", e);
        }
    }

    match provider_impl.get_transaction_count(wallet_address).await {
        Ok(nonce) => {
            tracing::info!("Wallet nonce: {}", nonce);
        }
        Err(e) => {
            tracing::warn!("Failed to get wallet nonce: {}", e);
        }
    }

    let provider = Arc::new(provider_impl);

    let app_state = AppState {
        provider,
        wallet_address,
        beacon_abi,
        beacon_factory_abi,
        beacon_registry_abi,
        perp_hook_abi,
        beacon_factory_address,
        perpcity_registry_address,
        perp_hook_address,
        access_token,
    };

    rocket::build().manage(app_state).mount(
        "/",
        rocket::routes![
            routes::index,
            routes::all_beacons,
            routes::create_beacon,
            routes::register_beacon,
            routes::create_perpcity_beacon,
            routes::batch_create_perpcity_beacon,
            routes::deploy_perp_for_beacon_endpoint,
            routes::batch_deploy_perps_for_beacons,
            routes::deposit_liquidity_for_perp_endpoint,
            routes::batch_deposit_liquidity_for_perps,
            routes::update_beacon
        ],
    )
}
