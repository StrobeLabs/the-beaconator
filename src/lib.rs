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

pub mod fairings;
pub mod guards;
pub mod models;
pub mod routes;

use crate::models::{AppState, PerpConfig};
use rocket::{Request, catch, catchers};

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

/// Load perp configuration from environment variables with fallback to defaults
fn load_perp_config() -> PerpConfig {
    let default_config = PerpConfig::default();

    let parse_env_or_default = |key: &str, default: u128| -> u128 {
        env::var(key)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(default)
    };

    let parse_env_or_default_i32 = |key: &str, default: i32| -> i32 {
        env::var(key)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(default)
    };

    let parse_env_or_default_i128 = |key: &str, default: i128| -> i128 {
        env::var(key)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(default)
    };

    let parse_env_or_default_u32 = |key: &str, default: u32| -> u32 {
        env::var(key)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(default)
    };

    PerpConfig {
        trading_fee_bps: parse_env_or_default_u32(
            "PERP_TRADING_FEE_BPS",
            default_config.trading_fee_bps,
        ),
        trading_fee_creator_split_x96: parse_env_or_default(
            "PERP_TRADING_FEE_CREATOR_SPLIT_X96",
            default_config.trading_fee_creator_split_x96,
        ),
        min_margin_usdc: parse_env_or_default(
            "PERP_MIN_MARGIN_USDC",
            default_config.min_margin_usdc,
        ),
        max_margin_usdc: parse_env_or_default(
            "PERP_MAX_MARGIN_USDC",
            default_config.max_margin_usdc,
        ),
        min_opening_leverage_x96: parse_env_or_default(
            "PERP_MIN_OPENING_LEVERAGE_X96",
            default_config.min_opening_leverage_x96,
        ),
        max_opening_leverage_x96: parse_env_or_default(
            "PERP_MAX_OPENING_LEVERAGE_X96",
            default_config.max_opening_leverage_x96,
        ),
        liquidation_leverage_x96: parse_env_or_default(
            "PERP_LIQUIDATION_LEVERAGE_X96",
            default_config.liquidation_leverage_x96,
        ),
        liquidation_fee_x96: parse_env_or_default(
            "PERP_LIQUIDATION_FEE_X96",
            default_config.liquidation_fee_x96,
        ),
        liquidation_fee_split_x96: parse_env_or_default(
            "PERP_LIQUIDATION_FEE_SPLIT_X96",
            default_config.liquidation_fee_split_x96,
        ),
        funding_interval_seconds: parse_env_or_default_i128(
            "PERP_FUNDING_INTERVAL_SECONDS",
            default_config.funding_interval_seconds,
        ),
        tick_spacing: parse_env_or_default_i32("PERP_TICK_SPACING", default_config.tick_spacing),
        starting_sqrt_price_x96: parse_env_or_default(
            "PERP_STARTING_SQRT_PRICE_X96",
            default_config.starting_sqrt_price_x96,
        ),
        default_tick_lower: parse_env_or_default_i32(
            "PERP_DEFAULT_TICK_LOWER",
            default_config.default_tick_lower,
        ),
        default_tick_upper: parse_env_or_default_i32(
            "PERP_DEFAULT_TICK_UPPER",
            default_config.default_tick_upper,
        ),
        liquidity_scaling_factor: parse_env_or_default(
            "PERP_LIQUIDITY_SCALING_FACTOR",
            default_config.liquidity_scaling_factor,
        ),
        max_margin_per_perp_usdc: parse_env_or_default(
            "PERP_MAX_MARGIN_PER_PERP_USDC",
            default_config.max_margin_per_perp_usdc,
        ),
    }
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

    let usdc_address = Address::from_str(
        &env::var("USDC_ADDRESS").expect("USDC_ADDRESS environment variable not set"),
    )
    .expect("Failed to parse USDC address");

    let usdc_transfer_limit = env::var("USDC_TRANSFER_LIMIT")
        .unwrap_or_else(|_| "1000000000".to_string()) // Default 1000 USDC
        .parse::<u128>()
        .expect("Failed to parse USDC_TRANSFER_LIMIT");

    let eth_transfer_limit = env::var("ETH_TRANSFER_LIMIT")
        .unwrap_or_else(|_| "10000000000000000".to_string()) // Default 0.01 ETH
        .parse::<u128>()
        .expect("Failed to parse ETH_TRANSFER_LIMIT");

    // Load perp configuration
    let perp_config = load_perp_config();

    // Log loaded configuration for debugging
    tracing::info!("Perp configuration loaded:");
    tracing::info!(
        "  - Trading fee: {}bps ({}%)",
        perp_config.trading_fee_bps,
        perp_config.trading_fee_bps as f64 / 100.0
    );
    tracing::info!(
        "  - Max margin: {} USDC",
        perp_config.max_margin_usdc as f64 / 1_000_000.0
    );
    tracing::info!("  - Tick spacing: {}", perp_config.tick_spacing);
    tracing::info!(
        "  - Funding interval: {}s ({}h)",
        perp_config.funding_interval_seconds,
        perp_config.funding_interval_seconds / 3600
    );
    tracing::info!(
        "  - Max margin per perp: {} USDC",
        perp_config.max_margin_per_perp_usdc as f64 / 1_000_000.0
    );

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
        usdc_address,
        usdc_transfer_limit,
        eth_transfer_limit,
        access_token,
        perp_config,
    };

    rocket::build()
        .manage(app_state)
        .attach(fairings::RequestLogger)
        .attach(fairings::PanicCatcher)
        .mount(
            "/",
            rocket::routes![
                routes::index,
                routes::all_beacons,
                routes::create_beacon,
                routes::register_beacon,
                routes::create_perpcity_beacon,
                routes::batch_create_perpcity_beacon,
                routes::deploy_perp_for_beacon_endpoint,
                routes::deposit_liquidity_for_perp_endpoint,
                routes::batch_deposit_liquidity_for_perps,
                routes::update_beacon,
                routes::fund_guest_wallet
            ],
        )
        .register("/", catchers![catch_all_errors, catch_panic])
}

#[catch(default)]
fn catch_all_errors(status: rocket::http::Status, request: &Request) -> String {
    let error_msg = format!(
        "Error {}: {} {}",
        status.code,
        request.method(),
        request.uri()
    );

    tracing::error!("Unhandled error: {}", error_msg);
    sentry::capture_message(&error_msg, sentry::Level::Error);

    format!(
        "Error {}: {}",
        status.code,
        status.reason().unwrap_or("Unknown error")
    )
}

#[catch(500)]
fn catch_panic(request: &Request) -> String {
    let error_msg = format!(
        "Internal Server Error (possible panic): {} {}",
        request.method(),
        request.uri()
    );

    tracing::error!("{}", error_msg);
    sentry::capture_message(&error_msg, sentry::Level::Fatal);

    "Internal Server Error".to_string()
}
