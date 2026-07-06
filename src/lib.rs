use alloy::{
    primitives::{Address, Bytes},
    signers::{Signer, aws::AwsSigner, local::PrivateKeySigner},
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

use crate::models::beacon_type::{BeaconTypeConfig, FactoryType};
use crate::models::wallet::WalletManagerConfig;
use crate::models::{
    AppState, AuthConfig, ContractAddresses, ProviderConfig, Registries, SafeConfig, WalletConfig,
};
use crate::services::beacon::BeaconTypeRegistry;
use crate::services::beacon::ComponentFactoryRegistry;
use crate::services::beacon::RecipeRegistry;
use crate::services::wallet::{BalanceTracker, PoolSigner, WalletManager, WalletSyncService};
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

/// Liveness probe for container orchestrators (ECS health checks, ALB).
///
/// No auth, no Redis, no RPC — returns 200 as long as the Rocket worker is
/// serving requests. Per-request logging for this path is suppressed in the
/// RequestLogger fairing so health checks don't spam the logs.
#[rocket::get("/health")]
fn health() -> (rocket::http::ContentType, &'static str) {
    (rocket::http::ContentType::JSON, r#"{"status":"ok"}"#)
}

/// Creates and configures the Rocket application.
///
/// Initializes the application state, loads configuration from environment variables,
/// sets up providers and wallets, and mounts all routes.
/// Pre-flight audit of every env var the-beaconator reads at startup.
///
/// Validates each variable WITHOUT logging its value. The only log output is a single
/// summary line at the end and one ERROR line per problem detected. This gives the
/// operator a list of things to fix on a fresh boot without echoing any config
/// (sensitive or otherwise) to the logs.
///
/// Detection rules:
/// - Required var missing → ERROR `<KEY> is required but not set`.
/// - Value contains leading/trailing space → ERROR with `raw_len` and `trimmed_len` only.
/// - Address-typed var fails `Address::from_str` → ERROR with the alloy error class only,
///   never the offending characters.
/// - `PRIVATE_KEY` length not 64 / 66 → ERROR with the observed length only.
/// - `WALLET_PRIVATE_KEYS` entry length not 64 / 66 → ERROR with index and observed
///   length only.
///
/// Anything that passes is silent. Lengths and integer error metadata are emitted because
/// they're necessary to fix the bug; raw values, addresses, URLs, and any portion of a
/// secret are NEVER emitted.
fn audit_environment() {
    use std::env;
    use std::str::FromStr;

    // Categorise every env var the-beaconator reads. ADD NEW ENTRIES HERE whenever a new
    // env var is plumbed in src/lib.rs — keeping the audit in sync with reality is the
    // whole point.
    const ADDRESS_VARS_REQUIRED: &[&str] = &[
        // Beacons system (beacons@v0.0.1)
        "PERPCITY_REGISTRY_ADDRESS",
        "ECDSA_VERIFIER_FACTORY_ADDRESS",
        // Perps system (perpcity-contracts@v0.1.0)
        "PERP_FACTORY_ADDRESS",
        // Per-perp Modules struct passed into PerpFactory.createPerp
        "FEES_MODULE_ADDRESS",
        "FUNDING_MODULE_ADDRESS",
        "MARGIN_RATIOS_MODULE_ADDRESS",
        "PRICE_IMPACT_MODULE_ADDRESS",
        "PRICING_MODULE_ADDRESS",
        // Tokens / utility
        "USDC_ADDRESS",
    ];
    const ADDRESS_VARS_OPTIONAL: &[&str] = &[
        "MULTICALL3_ADDRESS",
        "LBCGBM_FACTORY_ADDRESS",
        "WEIGHTED_SUM_COMPOSITE_FACTORY_ADDRESS",
        "SAFE_ADDRESS",
        // Governance / diagnostic; not on the deploy/open path
        "PROTOCOL_FEE_MANAGER_ADDRESS",
        "MODULE_REGISTRY_ADDRESS",
    ];
    const SECRET_VARS_REQUIRED: &[&str] = &[
        "RPC_URL",
        "PRIVATE_KEY",
        "BEACONATOR_ACCESS_TOKEN",
        "BEACONATOR_ADMIN_TOKEN",
        "REDIS_URL",
    ];
    // The wallet pool takes exactly one of WALLET_KMS_KEY_IDS /
    // WALLET_KMS_ALIAS_PREFIX / WALLET_PRIVATE_KEYS (checked separately below),
    // so none is individually required.
    const SECRET_VARS_OPTIONAL: &[&str] = &[
        "SAFE_TX_SERVICE_URL",
        "WALLET_PRIVATE_KEYS",
        "WALLET_KMS_KEY_IDS",
        "WALLET_KMS_ALIAS_PREFIX",
        // perpcity-bot-api key for the touch-on-update beacon->perps lookup
        // (src/services/touch). Only needed when TOUCH_ON_UPDATE_ENABLED.
        "BOT_API_KEY",
    ];
    // Other env vars the-beaconator reads. We don't log their values either; we only
    // check presence (for required) and whitespace cleanliness.
    const OTHER_VARS_REQUIRED: &[&str] = &["ENV"];
    const OTHER_VARS_OPTIONAL: &[&str] = &[
        "USDC_TRANSFER_LIMIT",
        "ETH_TRANSFER_LIMIT",
        "USDC_BONUS_LIMIT",
        "BEACONATOR_INSTANCE_ID",
        "RUST_LOG",
        // JSON map of component factory addresses seeded into Redis at startup
        // (set by the AWS deployment; see perpcity-client/sst.config.ts)
        "COMPONENT_FACTORIES_JSON",
        // Wallet pool balance sweep (src/services/wallet/balances.rs): ETH floor
        // (wei) below which a pool wallet is flagged + skipped by proactive
        // selection, and how often the sweep refreshes cached balances.
        "WALLET_MIN_ETH_WEI",
        "WALLET_BALANCE_SWEEP_SECS",
        // Touch-on-update side-loop (src/services/touch). All optional; the
        // feature is off unless TOUCH_ON_UPDATE_ENABLED is truthy, and BOT_API_URL
        // + BOT_API_KEY + MULTICALL3_ADDRESS are then required (checked at spawn).
        "TOUCH_ON_UPDATE_ENABLED",
        "BOT_API_URL",
        "TOUCH_FLUSH_INTERVAL_MS",
        "TOUCH_MAX_BATCH",
        "TOUCH_MAPPING_TTL_SECONDS",
        "TOUCH_MAPPING_EMPTY_TTL_SECONDS",
    ];

    let mut problems = 0usize;

    // Required presence + whitespace checks (no value logging).
    for &key in ADDRESS_VARS_REQUIRED
        .iter()
        .chain(SECRET_VARS_REQUIRED.iter())
        .chain(OTHER_VARS_REQUIRED.iter())
    {
        match env::var(key) {
            Ok(raw) => {
                if raw.len() != raw.trim().len() {
                    tracing::error!(
                        "{key} has hidden leading/trailing whitespace (raw_len={}, trimmed_len={})",
                        raw.len(),
                        raw.trim().len()
                    );
                    problems += 1;
                }
            }
            Err(_) => {
                tracing::error!("{key} is required but not set");
                problems += 1;
            }
        }
    }

    // Optional vars: only check whitespace if present. Missing is silent.
    for &key in ADDRESS_VARS_OPTIONAL
        .iter()
        .chain(SECRET_VARS_OPTIONAL.iter())
        .chain(OTHER_VARS_OPTIONAL.iter())
    {
        if let Ok(raw) = env::var(key)
            && raw.len() != raw.trim().len()
        {
            tracing::error!(
                "{key} has hidden leading/trailing whitespace (raw_len={}, trimmed_len={})",
                raw.len(),
                raw.trim().len()
            );
            problems += 1;
        }
    }

    // Address-typed vars: validate parse without logging the value or the offending
    // characters. The Address::from_str error class (e.g. "invalid string length") is
    // safe to log; it doesn't echo the raw input.
    for &key in ADDRESS_VARS_REQUIRED
        .iter()
        .chain(ADDRESS_VARS_OPTIONAL.iter())
    {
        if let Ok(raw) = env::var(key)
            && let Err(e) = Address::from_str(raw.trim())
        {
            tracing::error!("{key} does not parse as Address: {e}");
            problems += 1;
        }
    }

    // PRIVATE_KEY: must be 64 (raw hex) or 66 (with 0x prefix) characters. We log only
    // the observed length, never any portion of the value.
    if let Ok(v) = env::var("PRIVATE_KEY") {
        let len = v.trim().len();
        if len != 64 && len != 66 {
            tracing::error!(
                "PRIVATE_KEY length is {len} after trim, expected 64 or 66; parse WILL fail"
            );
            problems += 1;
        }
    }

    // WALLET_PRIVATE_KEYS: comma-separated list of keys, each must be 64 or 66 chars.
    // We log only the index and length per malformed entry, never any portion of the
    // value. This is how we caught WALLET_PRIVATE_KEYS[3] = a 42-char address on
    // 2026-05-29.
    if let Ok(v) = env::var("WALLET_PRIVATE_KEYS") {
        for (i, raw) in v.split(',').enumerate() {
            let len = raw.trim().len();
            if len != 0 && len != 64 && len != 66 {
                tracing::error!(
                    "WALLET_PRIVATE_KEYS[{i}] length is {len} after trim, expected 64 or 66; \
                     parse WILL fail"
                );
                problems += 1;
            }
        }
    }

    // Wallet pool source: exactly one of the three vars must be set. (KMS vars
    // carry key ids/aliases, not secrets, but the pool cannot start without one.)
    if env::var("WALLET_KMS_KEY_IDS").is_err()
        && env::var("WALLET_KMS_ALIAS_PREFIX").is_err()
        && env::var("WALLET_PRIVATE_KEYS").is_err()
    {
        tracing::error!(
            "wallet pool source missing: set one of WALLET_KMS_KEY_IDS, \
             WALLET_KMS_ALIAS_PREFIX, or WALLET_PRIVATE_KEYS"
        );
        problems += 1;
    }

    if problems == 0 {
        tracing::info!("Pre-flight environment audit: all checks passed");
    } else {
        tracing::error!(
            "Pre-flight environment audit: {problems} problem(s) detected; startup will likely fail"
        );
    }
}

/// Discover gas-payer wallet keys by KMS alias prefix (e.g.
/// "alias/perpcity/testnet/wallet-") via kms:ListAliases. Returns the matching
/// alias names, sorted for deterministic pool ordering. Aliases without a
/// target key are skipped. Requires kms:ListAliases on the caller's role.
async fn discover_wallet_aliases(client: &aws_sdk_kms::Client, prefix: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    let mut pages = client.list_aliases().into_paginator().send();
    while let Some(page) = pages.next().await {
        let page = page.unwrap_or_else(|e| panic!("kms:ListAliases failed: {e}"));
        for entry in page.aliases() {
            let Some(name) = entry.alias_name() else {
                continue;
            };
            if name.starts_with(prefix) && entry.target_key_id().is_some() {
                aliases.push(name.to_string());
            }
        }
    }
    aliases.sort();
    aliases
}

pub async fn create_rocket() -> Rocket<Build> {
    // Load and cache environment variables
    dotenvy::dotenv().ok();

    // Verbose pre-flight audit of every env var the-beaconator reads. Runs BEFORE any
    // parse attempt so the operator can see every problem in one log dump even when the
    // next step is going to panic. Secrets are never logged in plaintext (only lengths +
    // whitespace warnings). See `audit_environment` above.
    audit_environment();

    // Load RPC configuration from environment
    let rpc_config = services::rpc::RpcConfig::from_env()
        .unwrap_or_else(|e| panic!("Failed to load RPC configuration: {e}"));

    let access_token = env::var("BEACONATOR_ACCESS_TOKEN")
        .expect("BEACONATOR_ACCESS_TOKEN environment variable not set");

    // Load contract addresses
    let perpcity_registry_address = Address::from_str(
        &env::var("PERPCITY_REGISTRY_ADDRESS")
            .expect("PERPCITY_REGISTRY_ADDRESS environment variable not set"),
    )
    .expect("Failed to parse perpcity registry address");

    // PerpFactory deploys per-market `Perp` contracts. v0.1.0 architecture.
    let perp_factory_address = Address::from_str(
        &env::var("PERP_FACTORY_ADDRESS")
            .expect("PERP_FACTORY_ADDRESS environment variable not set"),
    )
    .expect("Failed to parse perp factory address");

    // Module addresses for the v0.1.0 perp Modules struct. All required at startup so
    // /deploy_perp_for_beacon never has to ask the caller for them.
    let parse_module_addr = |key: &str| -> Address {
        Address::from_str(
            &env::var(key).unwrap_or_else(|_| panic!("{key} environment variable not set")),
        )
        .unwrap_or_else(|e| panic!("Failed to parse {key}: {e}"))
    };
    let fees_module_address = parse_module_addr("FEES_MODULE_ADDRESS");
    let funding_module_address = parse_module_addr("FUNDING_MODULE_ADDRESS");
    let margin_ratios_module_address = parse_module_addr("MARGIN_RATIOS_MODULE_ADDRESS");
    let price_impact_module_address = parse_module_addr("PRICE_IMPACT_MODULE_ADDRESS");
    let pricing_module_address = parse_module_addr("PRICING_MODULE_ADDRESS");

    // Optional governance / diagnostic addresses — not on the deploy path.
    let parse_optional_addr = |key: &str| -> Option<Address> {
        env::var(key).ok().and_then(|s| {
            Address::from_str(&s)
                .map_err(|e| tracing::warn!("Invalid {} '{}': {}", key, s, e))
                .ok()
        })
    };
    let protocol_fee_manager_address = parse_optional_addr("PROTOCOL_FEE_MANAGER_ADDRESS");
    let module_registry_address = parse_optional_addr("MODULE_REGISTRY_ADDRESS");

    let usdc_address = Address::from_str(
        &env::var("USDC_ADDRESS").expect("USDC_ADDRESS environment variable not set"),
    )
    .expect("Failed to parse USDC address");

    // Optional multicall3 address for batch operations
    let multicall3_address = env::var("MULTICALL3_ADDRESS").ok().and_then(|addr_str| {
        Address::from_str(&addr_str)
            .map_err(|e| tracing::warn!("Invalid MULTICALL3_ADDRESS '{}': {}", addr_str, e))
            .ok()
    });

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
    let lbcgbm_factory_address = env::var("LBCGBM_FACTORY_ADDRESS").ok().and_then(|s| {
        Address::from_str(&s)
            .map_err(|e| tracing::warn!("Invalid LBCGBM_FACTORY_ADDRESS '{}': {}", s, e))
            .ok()
    });

    if let Some(addr) = lbcgbm_factory_address {
        tracing::info!("LBCGBM factory address: {:?}", addr);
    }

    let weighted_sum_composite_factory_address = env::var("WEIGHTED_SUM_COMPOSITE_FACTORY_ADDRESS")
        .ok()
        .and_then(|s| {
            Address::from_str(&s)
                .map_err(|e| {
                    tracing::warn!(
                        "Invalid WEIGHTED_SUM_COMPOSITE_FACTORY_ADDRESS '{}': {}",
                        s,
                        e
                    )
                })
                .ok()
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

    let usdc_bonus_limit = env::var("USDC_BONUS_LIMIT")
        .unwrap_or_else(|_| "50000000".to_string()) // Default 50 USDC
        .parse::<u128>()
        .expect("Failed to parse USDC_BONUS_LIMIT");

    // Get environment configuration and chain ID
    let env_type = &rpc_config.env_type;
    let chain_id = match env_type.to_lowercase().as_str() {
        "testnet" => 421614u64,  // Arbitrum Sepolia
        "mainnet" => 42161u64,   // Arbitrum One
        "localnet" => 421614u64, // Use testnet chain ID for local development/CI
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

    // Parse the measurement signer private key. This signer ONLY signs EIP-712
    // digests for ECDSA beacon updates — it never holds or sends funds. All
    // on-chain sends (gas + guest funding transfers) go through the KMS-capable
    // pool wallets configured below.
    let private_key = env::var("PRIVATE_KEY").expect("PRIVATE_KEY environment variable not set");

    // Get measurement signer address
    let signer_address = services::rpc::RpcConfig::get_wallet_address(&private_key)
        .expect("Failed to get measurement signer address");

    // Parse the private key into a signer for ECDSA operations
    let signer = private_key
        .parse::<PrivateKeySigner>()
        .expect("Failed to parse private key into signer")
        .with_chain_id(Some(chain_id));

    // Log measurement signer configuration. No balance check here by design: this
    // signer holds no funds — the pool wallets carry the float for gas and guest
    // funding transfers.
    tracing::info!("Measurement signer configured (EIP-712 signing only, holds no funds):");
    tracing::info!("  - Address: {:?}", signer_address);
    tracing::info!("  - Chain ID: {:?}", chain_id);
    tracing::info!("  - ENV: {}", env_type);

    // Build the gas-payer pool signers, in precedence order:
    //   1. WALLET_KMS_KEY_IDS - explicit comma-separated KMS key ids / aliases / ARNs.
    //   2. WALLET_KMS_ALIAS_PREFIX - discover pool keys by KMS alias prefix
    //      (e.g. "alias/perpcity/testnet/wallet-") via kms:ListAliases. Expanding
    //      the pool is then just `kms-wallet create` + fund + service restart:
    //      no env or IAM change when the grant uses a kms:RequestAlias condition.
    //   3. WALLET_PRIVATE_KEYS - comma-separated raw keys (dev/CI, no KMS access).
    // For 1 and 2 the private key never leaves KMS.
    let explicit_kms_ids = env::var("WALLET_KMS_KEY_IDS").ok();
    let kms_alias_prefix = env::var("WALLET_KMS_ALIAS_PREFIX").ok();
    let pool_signers: Vec<PoolSigner> = if explicit_kms_ids.is_some() || kms_alias_prefix.is_some()
    {
        // aws-config resolves credentials from the standard chain (the ECS task
        // role on Fargate); one shared KMS client is reused across pool signers.
        let aws_cfg = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let kms_client = aws_sdk_kms::Client::new(&aws_cfg);

        let (ids, source): (Vec<String>, &str) = if let Some(kms_ids) = explicit_kms_ids {
            let ids: Vec<String> = kms_ids
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
            // Fail fast on a present-but-blank value ("", ","), which would
            // otherwise boot the service with an empty wallet pool.
            if ids.is_empty() {
                panic!("WALLET_KMS_KEY_IDS is set but contains no usable KMS key ids");
            }
            (ids, "WALLET_KMS_KEY_IDS")
        } else {
            let prefix = kms_alias_prefix.unwrap();
            let prefix = prefix.trim();
            // A blank prefix would starts_with-match EVERY alias in the account.
            if prefix.is_empty() {
                panic!("WALLET_KMS_ALIAS_PREFIX is set but blank");
            }
            let ids = discover_wallet_aliases(&kms_client, prefix).await;
            if ids.is_empty() {
                panic!("WALLET_KMS_ALIAS_PREFIX '{prefix}' matched no KMS aliases");
            }
            (ids, "WALLET_KMS_ALIAS_PREFIX")
        };

        let mut signers = Vec::new();
        for id in &ids {
            let signer = AwsSigner::new(kms_client.clone(), id.clone(), Some(chain_id))
                .await
                .unwrap_or_else(|e| {
                    panic!("Failed to build AwsSigner for {source} entry '{id}': {e}")
                });
            tracing::info!("Pool wallet {} <- {id} (KMS)", signer.address());
            signers.push(PoolSigner::Kms(signer));
        }
        tracing::info!(
            "Loaded {} wallet signers from {source} (KMS)",
            signers.len()
        );
        signers
    } else {
        let wallet_keys_str = env::var("WALLET_PRIVATE_KEYS").expect(
            "One of WALLET_KMS_KEY_IDS, WALLET_KMS_ALIAS_PREFIX, or WALLET_PRIVATE_KEYS must be set for the wallet pool",
        );
        let signers: Vec<PoolSigner> = wallet_keys_str
            .split(',')
            .map(|k| {
                PoolSigner::Local(
                    k.trim()
                        .parse::<PrivateKeySigner>()
                        .unwrap_or_else(|e| {
                            panic!("Invalid private key in WALLET_PRIVATE_KEYS: {e}")
                        })
                        .with_chain_id(Some(chain_id)),
                )
            })
            .collect();
        tracing::info!(
            "Loaded {} wallet signers from WALLET_PRIVATE_KEYS (local)",
            signers.len()
        );
        signers
    };

    // Pool addresses, derived once for the Redis sync below (works for both backends).
    let pool_addresses: Vec<Address> = pool_signers.iter().map(PoolSigner::address).collect();

    // Initialize WalletManager (REQUIRED for contract operations)
    let mut wallet_config = WalletManagerConfig::from_env().unwrap_or_else(|e| {
        panic!("WalletManager configuration is required: {e}. Required env vars: REDIS_URL")
    });
    let redis_url = wallet_config.redis_url.clone();

    // Set chain_id from the already-determined chain_id
    wallet_config.chain_id = Some(chain_id);

    let mut wallet_manager = WalletManager::new(wallet_config, pool_signers)
        .await
        .unwrap_or_else(|e| {
            panic!("WalletManager failed to initialize: {e}. Check Redis connectivity.")
        });

    tracing::info!("WalletManager initialized for contract operations");

    // Balance tracker: periodically refreshes cached ETH/USDC balances for the
    // pool so selection can proactively skip a wallet under the ETH floor and
    // funding routes can order by cached USDC, plus emits per-wallet CloudWatch
    // metrics. Attach it to the manager BEFORE it's shared behind the AppState
    // Arc below — selection reads it through that Arc from then on.
    let balance_tracker =
        std::sync::Arc::new(BalanceTracker::new(read_provider.clone(), usdc_address));
    wallet_manager.set_balance_tracker(std::sync::Arc::clone(&balance_tracker));
    let balance_sweep_interval = BalanceTracker::sweep_interval_from_env();
    balance_tracker.spawn_sweep(pool_addresses.clone(), balance_sweep_interval);
    tracing::info!(
        "Wallet balance sweep started (interval {:?}, {} wallet(s))",
        balance_sweep_interval,
        pool_addresses.len()
    );

    // Sync pool wallet addresses to Redis pool on startup
    let sync_service = WalletSyncService::new(&pool_addresses, wallet_manager.pool());
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

    // Initialize ComponentFactoryRegistry (Redis-backed)
    let component_factory_registry = ComponentFactoryRegistry::new(&redis_url)
        .await
        .unwrap_or_else(|e| {
            panic!("ComponentFactoryRegistry failed to initialize: {e}. Check Redis connectivity.")
        });

    // Seed factory addresses from COMPONENT_FACTORIES_JSON when provided (the AWS
    // deployment sets it because ElastiCache is VPC-internal and cannot be seeded by
    // hand the way the Railway Redis was). Existing entries are never overwritten, so
    // re-deploys and registry edits made through Redis stay intact.
    if let Ok(factories_json) = env::var("COMPONENT_FACTORIES_JSON") {
        let configs = models::component_factory::parse_component_factories_json(&factories_json)
            .unwrap_or_else(|e| panic!("COMPONENT_FACTORIES_JSON is invalid: {e}"));
        match component_factory_registry.seed_defaults(&configs).await {
            Ok(result) => {
                tracing::info!(
                    "Component factory seed: {} seeded, {} already existed",
                    result.seeded,
                    result.skipped
                );
            }
            Err(e) => {
                panic!("Failed to seed component factories from COMPONENT_FACTORIES_JSON: {e}");
            }
        }
    }

    match component_factory_registry.list_factories().await {
        Ok(factories) => {
            tracing::info!(
                "Component factory registry contains {} factories",
                factories.len()
            );
        }
        Err(e) => {
            tracing::warn!("Failed to list component factories: {e}");
        }
    }

    // Initialize RecipeRegistry and seed standard recipes (Redis-backed)
    let recipe_registry = RecipeRegistry::new(&redis_url).await.unwrap_or_else(|e| {
        panic!("RecipeRegistry failed to initialize: {e}. Check Redis connectivity.")
    });

    match recipe_registry.seed_standard_recipes().await {
        Ok(result) => {
            tracing::info!(
                "Recipe seed: {} seeded, {} already existed",
                result.seeded,
                result.skipped
            );
        }
        Err(e) => {
            tracing::warn!("Failed to seed standard recipes: {e}");
        }
    }

    // Validate that all enabled recipes have their required factories registered
    if let Ok(recipes) = recipe_registry.list_recipes().await {
        for recipe in recipes.iter().filter(|r| r.enabled) {
            for factory_type in recipe.beacon_kind.required_factory_types() {
                if component_factory_registry
                    .get_factory_address(&factory_type)
                    .await
                    .is_err()
                {
                    tracing::warn!(
                        "Recipe '{}' requires {} but it is not registered",
                        recipe.slug,
                        factory_type
                    );
                }
            }
        }
    }

    // Optional Safe multisig configuration for beacon registration
    let safe_config = env::var("SAFE_ADDRESS").ok().and_then(|addr_str| {
        let address = match Address::from_str(&addr_str) {
            Ok(addr) => addr,
            Err(e) => {
                tracing::warn!("Invalid SAFE_ADDRESS '{}': {}", addr_str, e);
                return None;
            }
        };
        let tx_service_url = env::var("SAFE_TX_SERVICE_URL")
            .ok()
            .or_else(|| services::safe::SafeTransactionService::default_url_for_chain(chain_id));
        if let Some(ref url) = tx_service_url {
            tracing::info!("Safe multisig configured:");
            tracing::info!("  - Safe address: {:?}", address);
            tracing::info!("  - TX Service URL: {}", url);
        }
        Some(SafeConfig {
            address,
            tx_service_url,
        })
    });

    // Share the wallet manager (behind an Arc) between AppState and the touch
    // worker. Wrapped here, after set_balance_tracker/sync, which need &mut/owned.
    let wallet_manager = std::sync::Arc::new(wallet_manager);

    // Best-effort funding refresh: touch() every perp backed by a beacon after a
    // confirmed ECDSA update. Feature-flagged (TOUCH_ON_UPDATE_ENABLED, default
    // off); a no-op dispatcher when disabled or misconfigured.
    let touch = services::touch::spawn_from_env(
        std::sync::Arc::clone(&wallet_manager),
        rpc_url.clone(),
        multicall3_address,
    );

    let app_state = AppState {
        provider: ProviderConfig {
            read_provider,
            rpc_url,
            chain_id,
        },
        wallets: WalletConfig {
            manager: wallet_manager,
            signer_address,
            signer,
            usdc_transfer_limit,
            eth_transfer_limit,
            usdc_bonus_limit,
        },
        contracts: ContractAddresses {
            perpcity_registry: perpcity_registry_address,
            perp_factory: perp_factory_address,
            usdc: usdc_address,
            ecdsa_verifier_factory: ecdsa_verifier_factory_address,
            multicall3: multicall3_address,
            identity_beacon_bytecode,
            safe: safe_config,
            fees_module: fees_module_address,
            funding_module: funding_module_address,
            margin_ratios_module: margin_ratios_module_address,
            price_impact_module: price_impact_module_address,
            pricing_module: pricing_module_address,
            protocol_fee_manager: protocol_fee_manager_address,
            module_registry: module_registry_address,
        },
        auth: AuthConfig {
            access_token,
            admin_token,
        },
        registries: Registries {
            beacon_types: std::sync::Arc::new(beacon_type_registry),
            component_factories: std::sync::Arc::new(component_factory_registry),
            recipes: std::sync::Arc::new(recipe_registry),
        },
        touch,
    };

    // Configure OpenAPI settings
    let openapi_settings = OpenApiSettings::new();

    // Generate routes and OpenAPI specification
    let (routes, openapi_spec) = openapi_get_routes_spec![
        openapi_settings:
        routes::info::index,
        routes::beacon::create_beacon,
        routes::beacon::create_beacon_with_ecdsa,
        routes::beacon::register_beacon,
        routes::beacon::update_beacon,
        routes::beacon::batch_update_beacon,
        routes::beacon::update_beacon_with_ecdsa_adapter,
        routes::beacon::create_lbcgbm_beacon_endpoint,
        routes::beacon::create_weighted_sum_composite_beacon_endpoint,
        routes::perp::deploy_perp_for_beacon_endpoint,
        routes::perp::deposit_liquidity_for_perp_endpoint,
        routes::wallet::fund_guest_wallet,
        routes::wallet::fund_bonus_wallet,
        routes::beacon_type::list_beacon_types,
        routes::beacon_type::get_beacon_type,
        routes::beacon_type::register_beacon_type,
        routes::beacon_type::update_beacon_type,
        routes::beacon_type::delete_beacon_type,
        routes::recipe::list_recipes,
        routes::recipe::get_recipe,
        routes::recipe::list_component_factories,
        routes::beacon::create_modular_beacon,
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
        .mount("/", rocket::routes![serve_openapi_spec, health])
        .manage(openapi_json)
        .register("/", catchers![catch_all_errors, catch_panic])
}

/// Catches all unhandled errors and returns a formatted error response.
///
/// Emits a structured tracing event (status_code/method/uri fields) so the 5xx
/// path can be filtered and aggregated in CloudWatch.
#[catch(default)]
fn catch_all_errors(status: rocket::http::Status, request: &Request) -> String {
    tracing::error!(
        status_code = status.code,
        method = %request.method(),
        uri = %request.uri(),
        "Unhandled error response"
    );

    format!(
        "Error {}: {}",
        status.code,
        status.reason().unwrap_or("Unknown error")
    )
}

/// Catches panic-related internal server errors.
///
/// Structured fields (status_code/method/uri) keep the 500 path aggregatable
/// in CloudWatch; this catcher is the single logging point for plain 500s.
#[catch(500)]
fn catch_panic(request: &Request) -> String {
    tracing::error!(
        status_code = 500,
        method = %request.method(),
        uri = %request.uri(),
        "Internal Server Error (possible panic)"
    );

    "Internal Server Error".to_string()
}
