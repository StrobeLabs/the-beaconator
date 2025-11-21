use alloy::{
    json_abi::JsonAbi,
    primitives::{Address, utils::format_ether},
    providers::Provider,
};
use rocket::{Build, Rocket};
use rocket_okapi::{openapi_get_routes_spec, settings::OpenApiSettings};
use std::env;
use std::str::FromStr;

pub mod fairings;
pub mod guards;
pub mod models;
pub mod routes;
pub mod services;

use crate::models::AppState;
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

/// Serves the OpenAPI JSON specification at /openapi.json
#[rocket::get("/openapi.json")]
fn serve_openapi_spec(
    openapi_json: &rocket::State<String>,
) -> (rocket::http::Status, (rocket::http::ContentType, String)) {
    (
        rocket::http::Status::Ok,
        (rocket::http::ContentType::JSON, openapi_json.to_string()),
    )
}

/// Creates and configures the Rocket application.
///
/// Initializes the application state, loads configuration from environment variables,
/// sets up providers and wallets, and mounts all routes.
pub async fn create_rocket() -> Rocket<Build> {
    // Load and cache environment variables
    dotenvy::dotenv().ok();

    // Load RPC configuration from environment
    let rpc_config = services::rpc::RpcConfig::from_env()
        .unwrap_or_else(|e| panic!("Failed to load RPC configuration: {e}"));

    let access_token = env::var("BEACONATOR_ACCESS_TOKEN")
        .expect("BEACONATOR_ACCESS_TOKEN environment variable not set");

    // Load ABIs from files
    let beacon_abi = load_abi("Beacon");
    let beacon_factory_abi = load_abi("BeaconFactory");
    let beacon_registry_abi = load_abi("BeaconRegistry");
    let perp_manager_abi = load_abi("PerpManager");
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

    let perp_manager_address = Address::from_str(
        &env::var("PERP_MANAGER_ADDRESS")
            .expect("PERP_MANAGER_ADDRESS environment variable not set"),
    )
    .expect("Failed to parse perp manager address");

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
                .inspect_err(|e| {
                    tracing::warn!(
                        "Failed to parse DICHOTOMOUS_BEACON_FACTORY_ADDRESS '{}': {}",
                        addr_str,
                        e
                    );
                })
                .ok()
        });

    if let Some(addr) = dichotomous_beacon_factory_address {
        tracing::info!("Dichotomous beacon factory address loaded: {:?}", addr);
    } else {
        tracing::info!(
            "DICHOTOMOUS_BEACON_FACTORY_ADDRESS not set - verifiable beacon route will be disabled"
        );
    }

    let usdc_transfer_limit = env::var("USDC_TRANSFER_LIMIT")
        .unwrap_or_else(|_| "1000000000".to_string()) // Default 1000 USDC
        .parse::<u128>()
        .expect("Failed to parse USDC_TRANSFER_LIMIT");

    let eth_transfer_limit = env::var("ETH_TRANSFER_LIMIT")
        .unwrap_or_else(|_| "10000000000000000".to_string()) // Default 0.01 ETH
        .parse::<u128>()
        .expect("Failed to parse ETH_TRANSFER_LIMIT");

    // Load perp module addresses
    let fees_module_address = Address::from_str(
        &env::var("FEES_MODULE_ADDRESS").expect("FEES_MODULE_ADDRESS environment variable not set"),
    )
    .expect("Failed to parse FEES_MODULE_ADDRESS");

    let margin_ratios_module_address = Address::from_str(
        &env::var("MARGIN_RATIOS_MODULE_ADDRESS")
            .expect("MARGIN_RATIOS_MODULE_ADDRESS environment variable not set"),
    )
    .expect("Failed to parse MARGIN_RATIOS_MODULE_ADDRESS");

    let lockup_period_module_address = Address::from_str(
        &env::var("LOCKUP_PERIOD_MODULE_ADDRESS")
            .expect("LOCKUP_PERIOD_MODULE_ADDRESS environment variable not set"),
    )
    .expect("Failed to parse LOCKUP_PERIOD_MODULE_ADDRESS");

    let sqrt_price_impact_limit_module_address = Address::from_str(
        &env::var("SQRT_PRICE_IMPACT_LIMIT_MODULE_ADDRESS")
            .expect("SQRT_PRICE_IMPACT_LIMIT_MODULE_ADDRESS environment variable not set"),
    )
    .expect("Failed to parse SQRT_PRICE_IMPACT_LIMIT_MODULE_ADDRESS");

    // Optional default starting price
    let default_starting_sqrt_price_x96 = env::var("PERP_DEFAULT_STARTING_SQRT_PRICE_X96")
        .ok()
        .and_then(|s| s.parse::<u128>().ok());

    // Log loaded module addresses for debugging
    tracing::info!("Perp module addresses loaded:");
    tracing::info!("  - Fees module: {:?}", fees_module_address);
    tracing::info!(
        "  - Margin ratios module: {:?}",
        margin_ratios_module_address
    );
    tracing::info!(
        "  - Lockup period module: {:?}",
        lockup_period_module_address
    );
    tracing::info!(
        "  - Price impact limit module: {:?}",
        sqrt_price_impact_limit_module_address
    );
    if let Some(price) = default_starting_sqrt_price_x96 {
        tracing::info!("  - Default starting sqrt price X96: {}", price);
    }

    // Get environment configuration and chain ID
    let env_type = &rpc_config.env_type;
    let chain_id = match env_type.to_lowercase().as_str() {
        "testnet" => 84532u64,  // Base Sepolia testnet
        "mainnet" => 8453u64,   // Base mainnet
        "localnet" => 84532u64, // Use testnet chain ID for local development/CI
        _ => panic!(
            "Invalid ENV value '{env_type}'. Must be either 'mainnet', 'testnet', or 'localnet'"
        ),
    };

    // Parse the wallet private key
    let private_key = env::var("PRIVATE_KEY").expect("PRIVATE_KEY environment variable not set");

    // Build RPC providers using the RPC module
    let rpc_providers = rpc_config
        .build_providers(&private_key, chain_id)
        .unwrap_or_else(|e| panic!("Failed to build RPC providers: {e}"));

    let provider = rpc_providers.primary;
    let alternate_provider = rpc_providers.alternate;

    // Get wallet address
    let wallet_address = services::rpc::RpcConfig::get_wallet_address(&private_key)
        .expect("Failed to get wallet address");

    // Log wallet configuration for debugging
    tracing::info!("Wallet configured:");
    tracing::info!("  - Address: {:?}", wallet_address);
    tracing::info!("  - Chain ID: {:?}", chain_id);
    tracing::info!("  - ENV: {}", env_type);

    // Check wallet balance and nonce for debugging
    match provider.get_balance(wallet_address).await {
        Ok(balance) => {
            tracing::info!("Wallet balance: {} ETH", format_ether(balance));
        }
        Err(e) => {
            tracing::warn!("Failed to get wallet balance: {}", e);
        }
    }

    match provider.get_transaction_count(wallet_address).await {
        Ok(nonce) => {
            tracing::info!("Wallet nonce: {}", nonce);
        }
        Err(e) => {
            tracing::warn!("Failed to get wallet nonce: {}", e);
        }
    }

    let app_state = AppState {
        provider,
        alternate_provider,
        wallet_address,
        beacon_abi,
        beacon_factory_abi,
        beacon_registry_abi,
        perp_manager_abi,
        multicall3_abi,
        dichotomous_beacon_factory_abi,
        step_beacon_abi,
        beacon_factory_address,
        perpcity_registry_address,
        perp_manager_address,
        usdc_address,
        dichotomous_beacon_factory_address,
        usdc_transfer_limit,
        eth_transfer_limit,
        access_token,
        fees_module_address,
        margin_ratios_module_address,
        lockup_period_module_address,
        sqrt_price_impact_limit_module_address,
        default_starting_sqrt_price_x96,
        multicall3_address,
    };

    // Configure OpenAPI settings
    let openapi_settings = OpenApiSettings::new();

    // Generate routes and OpenAPI specification
    // Note: All routes are included, including verifiable beacon route
    // The verifiable beacon route will return an error at runtime if factory address is not configured
    let (routes, openapi_spec) = openapi_get_routes_spec![
        openapi_settings:
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
        routes::beacon::create_verifiable_beacon,
    ];

    // Serve the OpenAPI spec at /openapi.json
    let openapi_json =
        serde_json::to_string(&openapi_spec).expect("Failed to serialize OpenAPI spec");

    // Create rocket instance with OpenAPI support
    rocket::build()
        .manage(app_state)
        .attach(fairings::RequestLogger)
        .attach(fairings::PanicCatcher)
        .mount("/", routes)
        .mount("/", rocket::routes![serve_openapi_spec])
        .manage(openapi_json)
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
