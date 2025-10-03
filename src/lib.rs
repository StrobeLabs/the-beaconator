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
pub mod services;

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

/// Loads a contract ABI from a JSON file.
///
/// Reads the ABI file from the `abis/` directory and parses it into a JsonAbi struct.
fn load_abi(name: &str) -> JsonAbi {
    let abi_path = format!("abis/{name}.json");
    let abi_content = std::fs::read_to_string(&abi_path)
        .unwrap_or_else(|_| panic!("Failed to read ABI file: {abi_path}"));
    serde_json::from_str(&abi_content)
        .unwrap_or_else(|_| panic!("Failed to parse ABI file: {abi_path}"))
}

/// Loads perp configuration from environment variables with fallback to defaults.
///
/// Reads perp-related configuration from environment variables, falling back to
/// default values if not specified.
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

/// Creates and configures the Rocket application.
///
/// Initializes the application state, loads configuration from environment variables,
/// sets up providers and wallets, and mounts all routes.
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
    let multicall3_abi = load_abi("Multicall3");
    let dichotomous_beacon_factory_abi = load_abi("DichotomousBeaconFactory");
    let step_beacon_abi = load_abi("StepBeacon");

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

    // Optional multicall3 address for batch operations
    let multicall3_address = env::var("MULTICALL3_ADDRESS")
        .ok()
        .map(|addr_str| Address::from_str(&addr_str).expect("Failed to parse MULTICALL3_ADDRESS"));

    if let Some(multicall_addr) = multicall3_address {
        tracing::info!("Multicall3 address configured: {:?}", multicall_addr);
    } else {
        tracing::warn!("MULTICALL3_ADDRESS not set - batch operations will be disabled");
    }

    // Load dichotomous beacon factory address from environment
    let dichotomous_beacon_factory_address = env::var("DICHOTOMOUS_BEACON_FACTORY_ADDRESS")
        .ok()
        .and_then(|addr_str| {
            Address::from_str(&addr_str)
                .map_err(|e| {
                    tracing::warn!("Failed to parse DICHOTOMOUS_BEACON_FACTORY_ADDRESS '{}': {}", addr_str, e);
                    e
                })
                .ok()
        });

    if let Some(addr) = dichotomous_beacon_factory_address {
        tracing::info!("Dichotomous beacon factory address loaded: {:?}", addr);
    } else {
        tracing::info!("DICHOTOMOUS_BEACON_FACTORY_ADDRESS not set - verifiable beacon route will be disabled");
    }

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

    // Validate perp configuration on startup
    if let Err(e) = perp_config.validate() {
        tracing::error!("PerpConfig validation failed: {}", e);
        panic!("Invalid PerpConfig: {e}");
    }

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

    // Setup alternate provider if BEACONATOR_ALTERNATE_RPC is provided
    let alternate_provider = if let Ok(alternate_rpc_url) = env::var("BEACONATOR_ALTERNATE_RPC") {
        tracing::info!("Setting up alternate RPC provider: {}", alternate_rpc_url);

        // Create alternate provider with same wallet
        let alternate_signer = private_key
            .parse::<PrivateKeySigner>()
            .expect("Failed to parse private key for alternate provider")
            .with_chain_id(Some(chain_id));

        let alternate_wallet = EthereumWallet::from(alternate_signer);

        let provider = ProviderBuilder::new()
            .wallet(alternate_wallet)
            .connect_http(
                alternate_rpc_url
                    .parse()
                    .expect("Invalid alternate RPC URL"),
            );
        tracing::info!("Alternate RPC provider setup successful");
        Some(Arc::new(provider))
    } else {
        tracing::info!("No alternate RPC configured (BEACONATOR_ALTERNATE_RPC not set)");
        None
    };

    let app_state = AppState {
        provider,
        alternate_provider,
        wallet_address,
        beacon_abi,
        beacon_factory_abi,
        beacon_registry_abi,
        perp_hook_abi,
        multicall3_abi,
        dichotomous_beacon_factory_abi,
        step_beacon_abi,
        beacon_factory_address,
        perpcity_registry_address,
        perp_hook_address,
        usdc_address,
        dichotomous_beacon_factory_address,
        usdc_transfer_limit,
        eth_transfer_limit,
        access_token,
        perp_config,
        multicall3_address,
    };

    let mut base_routes = rocket::routes![
        routes::info::index,
        routes::info::all_beacons,
        routes::beacon::create_beacon,
        routes::beacon::register_beacon,
        routes::beacon::create_perpcity_beacon,
        routes::beacon::batch_create_perpcity_beacon,
        routes::perp::deploy_perp_for_beacon_endpoint,
        routes::perp::batch_deploy_perps_for_beacons,
        routes::perp::deposit_liquidity_for_perp_endpoint,
        routes::perp::batch_deposit_liquidity_for_perps,
        routes::beacon::update_beacon,
        routes::beacon::batch_update_beacon,
        routes::wallet::fund_guest_wallet,
    ];

    // Only register verifiable beacon route if factory address is configured
    if dichotomous_beacon_factory_address.is_some() {
        base_routes.extend(rocket::routes![routes::beacon::create_verifiable_beacon]);
    }

    rocket::build()
        .manage(app_state)
        .attach(fairings::RequestLogger)
        .attach(fairings::PanicCatcher)
        .mount("/", base_routes)
        .register("/", catchers![catch_all_errors, catch_panic])
}

/// Catches all unhandled errors and returns a formatted error response.
///
/// Logs the error and sends it to Sentry for monitoring.
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

/// Catches panic-related internal server errors.
///
/// Logs the panic and sends it to Sentry with fatal level.
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
