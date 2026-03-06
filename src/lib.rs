use alloy::{
    json_abi::JsonAbi,
    primitives::{Address, Bytes, utils::format_ether},
    providers::Provider,
    signers::{Signer, local::PrivateKeySigner},
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
use crate::models::beacon_type::{BeaconTypeConfig, FactoryType};
use crate::models::wallet::WalletManagerConfig;
use crate::services::beacon::BeaconTypeRegistry;
use crate::services::wallet::{WalletManager, WalletSyncService};
use rocket::{Request, catch, catchers};

// Provider type with embedded wallet for signing transactions
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

// Read-only provider type without wallet (for queries only, cannot sign transactions)
pub type ReadOnlyProvider = alloy::providers::fillers::FillProvider<
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
    let beacon_registry_abi = load_abi("BeaconRegistry");
    let perp_manager_abi = load_abi("PerpManager");
    let multicall3_abi = load_abi("Multicall3");

    // Load contract addresses
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

    // Load ECDSA verifier factory address
    let ecdsa_verifier_factory_address = Address::from_str(
        &env::var("ECDSA_VERIFIER_FACTORY_ADDRESS")
            .expect("ECDSA_VERIFIER_FACTORY_ADDRESS environment variable not set"),
    )
    .expect("Failed to parse ECDSA verifier factory address");

    tracing::info!(
        "ECDSA verifier factory address: {:?}",
        ecdsa_verifier_factory_address
    );

    // Load optional factory addresses for other beacon types
    let lbcgbm_factory_address = env::var("LBCGBM_FACTORY_ADDRESS")
        .ok()
        .map(|s| Address::from_str(&s).expect("Failed to parse LBCGBM_FACTORY_ADDRESS"));

    if let Some(addr) = lbcgbm_factory_address {
        tracing::info!("LBCGBM factory address: {:?}", addr);
    }

    let weighted_sum_composite_factory_address = env::var("WEIGHTED_SUM_COMPOSITE_FACTORY_ADDRESS")
        .ok()
        .map(|s| {
            Address::from_str(&s).expect("Failed to parse WEIGHTED_SUM_COMPOSITE_FACTORY_ADDRESS")
        });

    if let Some(addr) = weighted_sum_composite_factory_address {
        tracing::info!("WeightedSumComposite factory address: {:?}", addr);
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

    // Get the RPC URL for storing in AppState (used by WalletHandle to build providers)
    let rpc_url = rpc_config.rpc_url().to_string();

    // Build read-only provider (no wallet, for queries only)
    let read_provider = std::sync::Arc::new(
        rpc_config
            .build_read_only_provider_from_config()
            .unwrap_or_else(|e| panic!("Failed to build read-only RPC provider: {e}")),
    );

    // Parse the funding wallet private key (ONLY for fund_guest_wallet endpoint)
    let private_key = env::var("PRIVATE_KEY").expect("PRIVATE_KEY environment variable not set");

    // Get funding wallet address
    let funding_wallet_address = services::rpc::RpcConfig::get_wallet_address(&private_key)
        .expect("Failed to get funding wallet address");

    // Parse the private key into a signer for ECDSA operations
    let signer = private_key
        .parse::<PrivateKeySigner>()
        .expect("Failed to parse private key into signer")
        .with_chain_id(Some(chain_id));

    // Log funding wallet configuration
    tracing::info!("Funding wallet configured (for fund_guest_wallet only):");
    tracing::info!("  - Address: {:?}", funding_wallet_address);
    tracing::info!("  - Chain ID: {:?}", chain_id);
    tracing::info!("  - ENV: {}", env_type);

    // Check funding wallet balance for debugging
    match read_provider.get_balance(funding_wallet_address).await {
        Ok(balance) => {
            tracing::info!("Funding wallet balance: {} ETH", format_ether(balance));
        }
        Err(e) => {
            tracing::warn!("Failed to get funding wallet balance: {}", e);
        }
    }

    // Parse wallet private keys from WALLET_PRIVATE_KEYS env var
    let wallet_keys_str =
        env::var("WALLET_PRIVATE_KEYS").expect("WALLET_PRIVATE_KEYS environment variable not set");
    let wallet_signers: Vec<PrivateKeySigner> = wallet_keys_str
        .split(',')
        .map(|k| {
            k.trim()
                .parse::<PrivateKeySigner>()
                .unwrap_or_else(|e| panic!("Invalid private key in WALLET_PRIVATE_KEYS: {e}"))
                .with_chain_id(Some(chain_id))
        })
        .collect();

    tracing::info!(
        "Loaded {} wallet signers from WALLET_PRIVATE_KEYS",
        wallet_signers.len()
    );

    // Initialize WalletManager (REQUIRED for contract operations)
    let mut wallet_config = WalletManagerConfig::from_env().unwrap_or_else(|e| {
        panic!("WalletManager configuration is required: {e}. Required env vars: REDIS_URL")
    });
    let redis_url = wallet_config.redis_url.clone();

    // Set chain_id from the already-determined chain_id
    wallet_config.chain_id = Some(chain_id);

    let wallet_manager = WalletManager::new(wallet_config, wallet_signers.clone())
        .await
        .unwrap_or_else(|e| {
            panic!("WalletManager failed to initialize: {e}. Check Redis connectivity.")
        });

    tracing::info!("WalletManager initialized for contract operations");

    // Sync local wallet signers to Redis pool on startup
    let sync_service = WalletSyncService::new(&wallet_signers, wallet_manager.pool());
    match sync_service.sync().await {
        Ok(result) => {
            tracing::info!(
                "Wallet sync completed: {} added, {} unchanged, {} errors",
                result.added.len(),
                result.unchanged.len(),
                result.errors.len()
            );
            for addr in &result.added {
                tracing::info!("  + Added wallet: {addr}");
            }
            for error in &result.errors {
                tracing::warn!("  ! Sync error: {error}");
            }
        }
        Err(e) => {
            tracing::warn!("Failed to sync wallets to pool: {e}");
        }
    }

    // Log wallet pool status
    match wallet_manager.list_wallets().await {
        Ok(wallets) => {
            tracing::info!("Wallet pool contains {} wallets", wallets.len());
            for wallet in &wallets {
                tracing::info!("  - {} ({:?})", wallet.address, wallet.status);
            }
        }
        Err(e) => {
            tracing::warn!("Failed to list wallets in pool: {}", e);
        }
    }

    // Load admin token
    let admin_token = env::var("BEACONATOR_ADMIN_TOKEN")
        .expect("BEACONATOR_ADMIN_TOKEN environment variable not set");

    // Load IdentityBeacon bytecode for on-chain deployment
    let identity_beacon_bytecode = {
        let bytecode_hex = std::fs::read_to_string("abis/IdentityBeacon.bytecode")
            .expect("Failed to read abis/IdentityBeacon.bytecode");
        let bytecode_hex = bytecode_hex
            .trim()
            .strip_prefix("0x")
            .unwrap_or(bytecode_hex.trim());
        let bytes =
            hex::decode(bytecode_hex).expect("Failed to decode IdentityBeacon bytecode hex");
        Bytes::from(bytes)
    };
    tracing::info!(
        "Loaded IdentityBeacon bytecode ({} bytes)",
        identity_beacon_bytecode.len()
    );

    // Initialize BeaconTypeRegistry (Redis-backed)
    let beacon_type_registry = BeaconTypeRegistry::new(&redis_url)
        .await
        .unwrap_or_else(|e| {
            panic!("BeaconTypeRegistry failed to initialize: {e}. Check Redis connectivity.")
        });

    // Seed default beacon types from env vars (only writes if slug doesn't exist)
    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut seed_configs = vec![BeaconTypeConfig {
        slug: "identity".to_string(),
        name: "Identity Beacon".to_string(),
        description: Some(
            "ECDSA-verified identity beacon that directly stores signed data as its index"
                .to_string(),
        ),
        factory_address: ecdsa_verifier_factory_address,
        factory_type: FactoryType::Identity,
        registry_address: Some(perpcity_registry_address),
        enabled: true,
        created_at: now_ts,
        updated_at: now_ts,
    }];

    if let Some(addr) = lbcgbm_factory_address {
        seed_configs.push(BeaconTypeConfig {
            slug: "lbcgbm".to_string(),
            name: "LBCGBM Standalone Beacon".to_string(),
            description: Some(
                "Standalone beacon with Identity preprocessor, CGBM base function, and Bounded transform"
                    .to_string(),
            ),
            factory_address: addr,
            factory_type: FactoryType::LBCGBM,
            registry_address: Some(perpcity_registry_address),
            enabled: true,
            created_at: now_ts,
            updated_at: now_ts,
        });
    }

    if let Some(addr) = weighted_sum_composite_factory_address {
        seed_configs.push(BeaconTypeConfig {
            slug: "weighted-sum-composite".to_string(),
            name: "Weighted Sum Composite Beacon".to_string(),
            description: Some(
                "Composite beacon that computes its index as a weighted sum of reference beacon indices"
                    .to_string(),
            ),
            factory_address: addr,
            factory_type: FactoryType::WeightedSumComposite,
            registry_address: Some(perpcity_registry_address),
            enabled: true,
            created_at: now_ts,
            updated_at: now_ts,
        });
    }

    match beacon_type_registry.seed_defaults(&seed_configs).await {
        Ok(result) => {
            tracing::info!(
                "Beacon type seed: {} seeded, {} already existed",
                result.seeded,
                result.skipped
            );
        }
        Err(e) => {
            tracing::warn!("Failed to seed beacon types: {e}");
        }
    }

    // Log registered beacon types
    match beacon_type_registry.list_types().await {
        Ok(types) => {
            tracing::info!("Beacon type registry contains {} types", types.len());
            for bt in &types {
                tracing::info!(
                    "  - {} ({:?}) factory={} enabled={}",
                    bt.slug,
                    bt.factory_type,
                    bt.factory_address,
                    bt.enabled
                );
            }
        }
        Err(e) => {
            tracing::warn!("Failed to list beacon types: {}", e);
        }
    }

    // Optional Safe multisig configuration for beacon registration
    let safe_address = env::var("SAFE_ADDRESS")
        .ok()
        .map(|addr_str| Address::from_str(&addr_str).expect("Failed to parse SAFE_ADDRESS"));

    let safe_tx_service_url = if let Some(safe_addr) = safe_address {
        let url = env::var("SAFE_TX_SERVICE_URL")
            .ok()
            .or_else(|| services::safe::SafeTransactionService::default_url_for_chain(chain_id));
        if let Some(ref url) = url {
            tracing::info!("Safe multisig configured:");
            tracing::info!("  - Safe address: {:?}", safe_addr);
            tracing::info!("  - TX Service URL: {}", url);
        }
        url
    } else {
        None
    };

    let app_state = AppState {
        // Provider
        read_provider,
        funding_wallet_address,
        wallet_manager: std::sync::Arc::new(wallet_manager),
        rpc_url,
        chain_id,
        signer,

        // ABIs
        beacon_registry_abi,
        perp_manager_abi,
        multicall3_abi,

        // Addresses
        perpcity_registry_address,
        perp_manager_address,
        usdc_address,
        ecdsa_verifier_factory_address,

        // Limits
        usdc_transfer_limit,
        eth_transfer_limit,

        // Auth
        access_token,
        admin_token,

        // Beacon type registry
        beacon_type_registry: std::sync::Arc::new(beacon_type_registry),

        // IdentityBeacon bytecode
        identity_beacon_bytecode,

        // Perp modules
        fees_module_address,
        margin_ratios_module_address,
        lockup_period_module_address,
        sqrt_price_impact_limit_module_address,
        multicall3_address,

        // Safe multisig
        safe_address,
        safe_tx_service_url,
    };

    // Configure OpenAPI settings
    let openapi_settings = OpenApiSettings::new();

    // Generate routes and OpenAPI specification
    let (routes, openapi_spec) = openapi_get_routes_spec![
        openapi_settings:
        routes::info::index,
        routes::info::all_beacons,
        routes::beacon::create_beacon,
        routes::beacon::create_beacon_with_ecdsa,
        routes::beacon::register_beacon,
        routes::beacon::update_beacon,
        routes::beacon::batch_update_beacon,
        routes::beacon::update_beacon_with_ecdsa_adapter,
        routes::perp::deploy_perp_for_beacon_endpoint,
        routes::perp::batch_deploy_perps_for_beacons,
        routes::perp::deposit_liquidity_for_perp_endpoint,
        routes::perp::batch_deposit_liquidity_for_perps,
        routes::wallet::fund_guest_wallet,
        routes::beacon_type::list_beacon_types,
        routes::beacon_type::get_beacon_type,
        routes::beacon_type::register_beacon_type,
        routes::beacon_type::update_beacon_type,
        routes::beacon_type::delete_beacon_type,
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
