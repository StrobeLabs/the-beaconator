use alloy::primitives::{Address, FixedBytes, keccak256};
use alloy::sol_types::SolValue;
use rocket::serde::json::Json;
use rocket::{State, http::Status, post};
use rocket_okapi::openapi;
use std::str::FromStr;
use tracing;

use crate::guards::ApiToken;
use crate::models::{
    ApiResponse, AppState, DeployPerpForBeaconRequest, DeployPerpForBeaconResponse,
    DepositLiquidityForPerpRequest, DepositLiquidityForPerpResponse,
};
use crate::routes::IPerpFactory;
use crate::services::perp::{deploy_perp_for_beacon, deposit_liquidity_for_perp};

/// Derive a deterministic 32-byte salt from the deploy request. Reusing this salt on retry
/// causes `LibClone.cloneDeterministic` inside PerpFactory.createPerp to revert if the previous
/// call already minted the accounting-token clones — making /deploy_perp_for_beacon idempotent
/// instead of silently creating a duplicate market when the client retries after a timeout.
///
/// Includes every user-controllable createPerp input so that distinct intents produce distinct
/// salts.
fn deterministic_salt(
    beacon: Address,
    owner: Address,
    name: &str,
    symbol: &str,
    token_uri: &str,
    ema_window: u32,
) -> FixedBytes<32> {
    let encoded = (
        beacon,
        owner,
        name.to_string(),
        symbol.to_string(),
        token_uri.to_string(),
        ema_window,
    )
        .abi_encode();
    keccak256(encoded)
}

/// Deploys a perpetual market contract for a specific beacon via PerpFactory.createPerp.
///
/// perpcity-contracts@v0.1.0 architecture: each market is its own `Perp` contract.
/// Module addresses (Fees / Funding / MarginRatios / PriceImpact / Pricing) are resolved
/// from the server's environment, not the request body.
#[openapi(tag = "Perpetual")]
#[post("/deploy_perp_for_beacon", data = "<request>")]
pub async fn deploy_perp_for_beacon_endpoint(
    request: Json<DeployPerpForBeaconRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<DeployPerpForBeaconResponse>>, Status> {
    tracing::info!("Received request: POST /deploy_perp_for_beacon");
    tracing::info!("Requested beacon address: {}", request.beacon_address);

    let beacon_address = match Address::from_str(&request.beacon_address) {
        Ok(addr) => addr,
        Err(e) => {
            let error_msg = format!("Invalid beacon address '{}': {}", request.beacon_address, e);
            tracing::error!("{}", error_msg);
            return Err(Status::BadRequest);
        }
    };

    let owner = match Address::from_str(&request.owner) {
        Ok(addr) => addr,
        Err(e) => {
            let error_msg = format!("Invalid owner address '{}': {}", request.owner, e);
            tracing::error!("{}", error_msg);
            return Err(Status::BadRequest);
        }
    };

    // Validate ema_window fits in uint24 and is non-zero (matches IPerpFactory.EmaWindowTooLow).
    // Defensive: also enforced inside deploy_perp_for_beacon, but rejecting here gives a clearer
    // BadRequest instead of a 500 from the service layer.
    if request.ema_window == 0 || request.ema_window > 0x00FF_FFFF {
        let error_msg = format!(
            "Invalid ema_window {}: must be in 1..=16777215 (uint24 non-zero)",
            request.ema_window
        );
        tracing::error!("{}", error_msg);
        return Err(Status::BadRequest);
    }

    let salt = match request.salt.as_deref() {
        None => deterministic_salt(
            beacon_address,
            owner,
            &request.name,
            &request.symbol,
            &request.token_uri,
            request.ema_window,
        ),
        Some(s) => match FixedBytes::<32>::from_str(s) {
            Ok(b) => b,
            Err(e) => {
                let error_msg = format!("Invalid salt '{s}': {e} (expected 32-byte hex)");
                tracing::error!("{}", error_msg);
                return Err(Status::BadRequest);
            }
        },
    };

    tracing::info!("Starting perp deployment process...");
    match deploy_perp_for_beacon(
        state,
        beacon_address,
        owner,
        request.name.clone(),
        request.symbol.clone(),
        request.token_uri.clone(),
        request.ema_window,
        salt,
    )
    .await
    {
        Ok(response) => {
            let message = "Perp deployed successfully!";
            tracing::info!("{}", message);
            tracing::info!("Perp address: {}", response.perp_address);
            tracing::info!("PerpFactory address: {}", response.perp_factory_address);
            tracing::info!("Pool ID: {}", response.pool_id);
            tracing::info!("Transaction hash: {}", response.transaction_hash);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(response),
                message: message.to_string(),
            }))
        }
        Err(e) => {
            let error_msg = format!("Failed to deploy perp for beacon {beacon_address}: {e}");
            tracing::error!("{}", error_msg);
            tracing::error!("Error context:");
            tracing::error!("  - Beacon address: {}", beacon_address);
            tracing::error!("  - PerpFactory address: {}", state.contracts.perp_factory);
            tracing::error!("  - USDC address: {}", state.contracts.usdc);

            Err(Status::InternalServerError)
        }
    }
}

/// Deposits liquidity (opens a maker position) on a per-market `Perp` contract.
///
/// Approves USDC spending against the per-Perp contract address and calls
/// `Perp.openMaker(OpenMakerParams)`. Returns the maker position ID and transaction hashes.
#[openapi(tag = "Perpetual")]
#[post("/deposit_liquidity_for_perp", data = "<request>")]
pub async fn deposit_liquidity_for_perp_endpoint(
    request: Json<DepositLiquidityForPerpRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<DepositLiquidityForPerpResponse>>, Status> {
    tracing::info!("Received request: POST /deposit_liquidity_for_perp");

    let perp_address = match Address::from_str(&request.perp_address) {
        Ok(addr) => addr,
        Err(e) => {
            let error_msg = format!("Invalid perp address '{}': {e}", request.perp_address);
            tracing::error!("{}", error_msg);
            return Err(Status::BadRequest);
        }
    };

    let margin_amount = match request.margin_amount_usdc.parse::<u128>() {
        Ok(amount) => amount,
        Err(e) => {
            let error_msg = format!(
                "Invalid margin amount '{}': {e}",
                request.margin_amount_usdc
            );
            tracing::error!("{}", error_msg);
            tracing::error!("Margin amount must be a valid number in USDC with 6 decimals");
            tracing::error!("  Examples: '1000000' = 1 USDC, '500000000' = 500 USDC");
            return Err(Status::BadRequest);
        }
    };

    tracing::info!(
        "Margin amount: {} USDC (validation delegated to on-chain modules)",
        margin_amount as f64 / 1_000_000.0
    );

    let tick_spacing = request.tick_spacing.unwrap_or(30);
    let tick_lower = request.tick_lower.unwrap_or(24390);
    let tick_upper = request.tick_upper.unwrap_or(53850);

    // Defense in depth: refuse to approve USDC against any address that wasn't deployed by the
    // trusted PerpFactory. The endpoint is gated by the API token, but a caller typo or a
    // compromised token must never produce a USDC allowance on an EOA or a non-Perp contract.
    //
    // The on-chain check is `PerpFactory.perps(address)` (boolean mapping populated in
    // createPerp). Run AFTER cheap input validation so 400-class errors are surfaced first.
    let factory = IPerpFactory::new(state.contracts.perp_factory, &state.provider.read_provider);
    match factory.perps(perp_address).call().await {
        Ok(is_known_perp) => {
            if !is_known_perp {
                let error_msg = format!(
                    "perp_address {perp_address} is not registered with PerpFactory \
                     {} — refusing to approve USDC to an untrusted address",
                    state.contracts.perp_factory
                );
                tracing::error!("{}", error_msg);
                return Err(Status::BadRequest);
            }
        }
        Err(e) => {
            let error_msg =
                format!("Failed to verify perp_address {perp_address} with factory: {e}");
            tracing::error!("{}", error_msg);
            return Err(Status::InternalServerError);
        }
    }

    match deposit_liquidity_for_perp(
        state,
        perp_address,
        margin_amount,
        tick_spacing,
        tick_lower,
        tick_upper,
    )
    .await
    {
        Ok(response) => {
            let message = "Liquidity deposited successfully";
            tracing::info!("{}", message);
            tracing::info!("Maker position ID: {}", response.maker_position_id);
            tracing::info!(
                "Approval transaction: {}",
                response.approval_transaction_hash
            );
            tracing::info!("Deposit transaction: {}", response.deposit_transaction_hash);
            Ok(Json(ApiResponse {
                success: true,
                data: Some(response),
                message: message.to_string(),
            }))
        }
        Err(e) => {
            let error_msg = format!(
                "Failed to deposit liquidity for perp {}: {e}",
                request.perp_address
            );
            tracing::error!("{}", error_msg);
            tracing::error!("Error context:");
            tracing::error!("  - Perp address: {}", request.perp_address);
            tracing::error!("  - Margin amount: {} USDC", request.margin_amount_usdc);
            tracing::error!("  - PerpFactory address: {}", state.contracts.perp_factory);

            Err(Status::InternalServerError)
        }
    }
}

// Tests moved to tests/unit_tests/perp_route_tests.rs
