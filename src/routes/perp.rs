use alloy::primitives::{Address, FixedBytes, U160};
use rocket::serde::json::Json;
use rocket::{State, http::Status, post};
use rocket_okapi::openapi;
use std::str::FromStr;
use tracing;

use super::execute_transaction_serialized;
use crate::guards::ApiToken;
use crate::models::{
    ApiResponse, AppState, BatchDeployPerpsForBeaconsRequest, BatchDeployPerpsForBeaconsResponse,
    BatchDepositLiquidityForPerpsRequest, BatchDepositLiquidityForPerpsResponse,
    DeployPerpForBeaconRequest, DeployPerpForBeaconResponse, DepositLiquidityForPerpRequest,
    DepositLiquidityForPerpResponse,
};
use crate::services::perp::{
    batch_deposit_liquidity_with_multicall3, deploy_perp_for_beacon, deposit_liquidity_for_perp,
    validate_module_address,
};

/// Deploys a perpetual contract for a specific beacon.
///
/// Creates a new perpetual pool using the PerpManager contract for the specified beacon address.
/// Returns the perp ID, PerpManager address, and transaction hash on success.
#[openapi(tag = "Perpetual")]
#[post("/deploy_perp_for_beacon", data = "<request>")]
pub async fn deploy_perp_for_beacon_endpoint(
    request: Json<DeployPerpForBeaconRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<DeployPerpForBeaconResponse>>, Status> {
    tracing::info!("Received request: POST /deploy_perp_for_beacon");
    tracing::info!("Requested beacon address: {}", request.beacon_address);

    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/deploy_perp_for_beacon");
        scope.set_extra("beacon_address", request.beacon_address.clone().into());
        scope.set_extra(
            "perp_manager_address",
            state.perp_manager_address.to_string().into(),
        );
        scope.set_extra("wallet_address", state.wallet_address.to_string().into());
    });

    // Parse the beacon address
    let beacon_address = match Address::from_str(&request.beacon_address) {
        Ok(addr) => addr,
        Err(e) => {
            let error_msg = format!("Invalid beacon address '{}': {}", request.beacon_address, e);
            tracing::error!("{}", error_msg);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            return Err(Status::BadRequest);
        }
    };

    // Parse module addresses
    let fees_module = match Address::from_str(&request.fees_module) {
        Ok(addr) => addr,
        Err(e) => {
            let error_msg = format!(
                "Invalid fees module address '{}': {}",
                request.fees_module, e
            );
            tracing::error!("{}", error_msg);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            return Err(Status::BadRequest);
        }
    };

    let margin_ratios_module = match Address::from_str(&request.margin_ratios_module) {
        Ok(addr) => addr,
        Err(e) => {
            let error_msg = format!(
                "Invalid margin ratios module address '{}': {}",
                request.margin_ratios_module, e
            );
            tracing::error!("{}", error_msg);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            return Err(Status::BadRequest);
        }
    };

    let lockup_period_module = match Address::from_str(&request.lockup_period_module) {
        Ok(addr) => addr,
        Err(e) => {
            let error_msg = format!(
                "Invalid lockup period module address '{}': {}",
                request.lockup_period_module, e
            );
            tracing::error!("{}", error_msg);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            return Err(Status::BadRequest);
        }
    };

    let sqrt_price_impact_limit_module =
        match Address::from_str(&request.sqrt_price_impact_limit_module) {
            Ok(addr) => addr,
            Err(e) => {
                let error_msg = format!(
                    "Invalid sqrt price impact limit module address '{}': {}",
                    request.sqrt_price_impact_limit_module, e
                );
                tracing::error!("{}", error_msg);
                sentry::capture_message(&error_msg, sentry::Level::Error);
                return Err(Status::BadRequest);
            }
        };

    // Validate all module addresses have deployed code
    tracing::info!("Validating module addresses...");

    if let Err(e) = validate_module_address(&state.provider, fees_module, "Fees module").await {
        sentry::capture_message(&e, sentry::Level::Error);
        return Err(Status::BadRequest);
    }

    if let Err(e) = validate_module_address(
        &state.provider,
        margin_ratios_module,
        "Margin ratios module",
    )
    .await
    {
        sentry::capture_message(&e, sentry::Level::Error);
        return Err(Status::BadRequest);
    }

    if let Err(e) = validate_module_address(
        &state.provider,
        lockup_period_module,
        "Lockup period module",
    )
    .await
    {
        sentry::capture_message(&e, sentry::Level::Error);
        return Err(Status::BadRequest);
    }

    if let Err(e) = validate_module_address(
        &state.provider,
        sqrt_price_impact_limit_module,
        "Sqrt price impact limit module",
    )
    .await
    {
        sentry::capture_message(&e, sentry::Level::Error);
        return Err(Status::BadRequest);
    }

    tracing::info!("All module addresses validated successfully");

    // Parse starting sqrt price
    let starting_sqrt_price_x96 = match U160::from_str(&request.starting_sqrt_price_x96) {
        Ok(price) => price,
        Err(e) => {
            let error_msg = format!(
                "Invalid starting sqrt price X96 '{}': {}",
                request.starting_sqrt_price_x96, e
            );
            tracing::error!("{}", error_msg);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            return Err(Status::BadRequest);
        }
    };

    tracing::info!("Starting perp deployment process...");
    match deploy_perp_for_beacon(
        state,
        beacon_address,
        fees_module,
        margin_ratios_module,
        lockup_period_module,
        sqrt_price_impact_limit_module,
        starting_sqrt_price_x96,
    )
    .await
    {
        Ok(response) => {
            let message = "Perp deployed successfully!";
            tracing::info!("{}", message);
            tracing::info!("Perp ID: {}", response.perp_id);
            tracing::info!("PerpManager address: {}", response.perp_manager_address);
            tracing::info!("Transaction hash: {}", response.transaction_hash);
            sentry::capture_message(
                &format!(
                    "Perp deployed successfully for beacon {beacon_address}, perp ID: {}",
                    response.perp_id
                ),
                sentry::Level::Info,
            );
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
            tracing::error!("  - PerpManager address: {}", state.perp_manager_address);
            tracing::error!("  - Wallet address: {}", state.wallet_address);
            tracing::error!("  - USDC address: {}", state.usdc_address);

            // Provide actionable next steps based on error
            tracing::error!("Recommended next steps:");
            if e.contains("execution reverted") {
                tracing::error!(
                    "  1. Verify PerpManager contract is deployed at {}",
                    state.perp_manager_address
                );
                tracing::error!(
                    "  2. Check beacon address {} exists and is valid",
                    beacon_address
                );
                tracing::error!(
                    "  3. Ensure external contracts (PoolManager, modules) are accessible"
                );
                tracing::error!("  4. Review module addresses and parameters for correctness");
            } else if e.contains("insufficient funds") {
                tracing::error!("  1. Check wallet balance and ensure sufficient ETH for gas");
                tracing::error!("  2. Verify USDC balance if contract requires token transfers");
            } else {
                tracing::error!("  1. Check network connectivity and RPC endpoint");
                tracing::error!("  2. Verify all contract addresses are correct");
                tracing::error!("  3. Try the request again after a short delay");
            }

            sentry::capture_message(&error_msg, sentry::Level::Error);
            Err(Status::InternalServerError)
        }
    }
}

/// Deposits liquidity for a specific perpetual contract.
///
/// Approves USDC spending and deposits the specified margin amount as liquidity
/// for the given perp ID. Returns the maker position ID and transaction hashes.
#[openapi(tag = "Perpetual")]
#[post("/deposit_liquidity_for_perp", data = "<request>")]
pub async fn deposit_liquidity_for_perp_endpoint(
    request: Json<DepositLiquidityForPerpRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<DepositLiquidityForPerpResponse>>, Status> {
    tracing::info!("Received request: POST /deposit_liquidity_for_perp");
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/deposit_liquidity_for_perp");
        scope.set_extra("perp_id", request.perp_id.clone().into());
        scope.set_extra("margin_amount", request.margin_amount_usdc.clone().into());
    });

    // Parse the perp ID (PoolId as bytes32)
    let perp_id = match FixedBytes::<32>::from_str(&request.perp_id) {
        Ok(id) => id,
        Err(e) => {
            let error_msg = format!("Invalid perp ID '{}': {e}", request.perp_id);
            tracing::error!("{}", error_msg);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            return Err(Status::BadRequest);
        }
    };

    // Parse the margin amount (USDC in 6 decimals)
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
            sentry::capture_message(&error_msg, sentry::Level::Error);
            return Err(Status::BadRequest);
        }
    };

    // All margin validations are performed by on-chain modules
    tracing::info!(
        "Margin amount: {} USDC (validation delegated to on-chain modules)",
        margin_amount as f64 / 1_000_000.0
    );

    // Extract tick parameters from request or use defaults
    let tick_spacing = request.tick_spacing.unwrap_or(30);
    let tick_lower = request.tick_lower.unwrap_or(24390);
    let tick_upper = request.tick_upper.unwrap_or(53850);

    match deposit_liquidity_for_perp(
        state,
        perp_id,
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
                request.perp_id
            );
            tracing::error!("{}", error_msg);
            tracing::error!("Error context:");
            tracing::error!("  - Perp ID: {}", request.perp_id);
            tracing::error!("  - Margin amount: {} USDC", request.margin_amount_usdc);
            tracing::error!("  - PerpManager address: {}", state.perp_manager_address);
            tracing::error!("  - Wallet address: {}", state.wallet_address);

            // Check for the specific unknown error 0xfb8f41b2 and provide detailed analysis
            if e.contains("0xfb8f41b2") {
                tracing::error!("Unknown contract error 0xfb8f41b2 detected");
                tracing::error!("   This error is NOT related to pool initialization");
                tracing::error!("   Error parameters suggest:");
                tracing::error!(
                    "     - Contract: {} (PerpManager)",
                    state.perp_manager_address
                );
                tracing::error!("     - Position/ID: 0 (may indicate new position)");
                tracing::error!("     - Amount: {} USDC", margin_amount as f64 / 1_000_000.0);
                tracing::error!("   Possible causes:");
                tracing::error!("     - Insufficient USDC balance or allowance");
                tracing::error!("     - Invalid perp configuration or state");
                tracing::error!("     - Contract access control or validation failure");
                tracing::error!("     - Custom business logic restriction in PerpManager");

                // Add specific troubleshooting for this error
                tracing::error!("   Troubleshooting steps:");
                tracing::error!(
                    "     1. Verify USDC balance for wallet: {}",
                    state.wallet_address
                );
                tracing::error!(
                    "     2. Check USDC allowance for PerpManager: {}",
                    state.perp_manager_address
                );
                tracing::error!(
                    "     3. Verify perp {} exists and is active",
                    request.perp_id
                );
                tracing::error!(
                    "     4. Check if margin amount {} USDC is within perp limits",
                    margin_amount as f64 / 1_000_000.0
                );
                tracing::error!("     5. Contact protocol team to identify this custom error");
            }

            // Provide actionable next steps
            tracing::error!("Recommended next steps:");
            if e.contains("execution reverted") {
                tracing::error!(
                    "  1. Verify perp ID {} exists and is active",
                    request.perp_id
                );
                tracing::error!(
                    "  2. Check margin amount {} is within allowed limits",
                    request.margin_amount_usdc
                );
                tracing::error!("  3. Ensure sufficient USDC balance for liquidity deposit");
                tracing::error!("  4. Verify tick range configuration is valid");
            } else if e.contains("invalid perp") || e.contains("perp not found") {
                tracing::error!("  1. Confirm perp was successfully deployed first");
                tracing::error!("  2. Verify perp ID format is correct (32-byte hex)");
                tracing::error!("  3. Check deployment transaction was confirmed");
            } else {
                tracing::error!("  1. Check network connectivity and RPC endpoint");
                tracing::error!("  2. Verify all contract addresses are correct");
                tracing::error!("  3. Try the request again after a short delay");
            }

            sentry::capture_message(&error_msg, sentry::Level::Error);
            Err(Status::InternalServerError)
        }
    }
}

/// Deposits liquidity for multiple perpetual contracts in a batch operation.
///
/// Processes multiple liquidity deposits, each with their own perp ID and margin amount.
/// Returns detailed results for each deposit attempt.
#[openapi(tag = "Perpetual")]
#[post("/batch_deposit_liquidity_for_perps", data = "<request>")]
pub async fn batch_deposit_liquidity_for_perps(
    request: Json<BatchDepositLiquidityForPerpsRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<BatchDepositLiquidityForPerpsResponse>>, Status> {
    tracing::info!("Received request: POST /batch_deposit_liquidity_for_perps");
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/batch_deposit_liquidity_for_perps");
        scope.set_extra("requested_count", request.liquidity_deposits.len().into());
    });

    let deposit_count = request.liquidity_deposits.len();

    // Validate the count (1-10 limit)
    if deposit_count == 0 || deposit_count > 10 {
        tracing::warn!("Invalid deposit count: {}", deposit_count);
        return Err(Status::BadRequest);
    }

    // Process all liquidity deposits in a single serialized transaction to avoid nonce conflicts
    let state_inner = state.inner();
    let deposits_clone = request.liquidity_deposits.clone();

    let batch_results = execute_transaction_serialized(async move {
        // Check if we have a multicall3 contract address configured
        if let Some(multicall_address) = state_inner.multicall3_address {
            // Use multicall3 for atomic batch liquidity deposits
            batch_deposit_liquidity_with_multicall3(state_inner, multicall_address, &deposits_clone)
                .await
        } else {
            // No multicall3 configured - return error for all deposits
            let error_msg =
                "Batch operations require Multicall3 contract address to be configured".to_string();
            tracing::error!("{}", error_msg);
            deposits_clone
                .iter()
                .map(|deposit| (deposit.perp_id.clone(), Err(error_msg.clone())))
                .collect()
        }
    })
    .await;

    // Process the results
    let mut maker_position_ids = Vec::new();
    let mut errors = Vec::new();

    for (_perp_id, result) in batch_results {
        match result {
            Ok(position_id) => {
                maker_position_ids.push(position_id);
            }
            Err(error) => {
                errors.push(error);
            }
        }
    }

    let deposited_count = maker_position_ids.len() as u32;
    let failed_count = deposit_count as u32 - deposited_count;

    let response_data = BatchDepositLiquidityForPerpsResponse {
        deposited_count,
        maker_position_ids: maker_position_ids.clone(),
        failed_count,
        errors,
    };

    let message = if failed_count == 0 {
        format!("Successfully deposited liquidity for all {deposited_count} perps")
    } else if deposited_count == 0 {
        "Failed to deposit any liquidity".to_string()
    } else {
        format!("Partially successful: {deposited_count} deposited, {failed_count} failed")
    };

    tracing::info!("{}", message);

    // Return success even with partial failures, let client handle the response
    Ok(Json(ApiResponse {
        success: deposited_count > 0,
        data: Some(response_data),
        message,
    }))
}

/// Deploys perpetual contracts for multiple beacons in a batch operation.
///
/// Creates perpetual pools for each specified beacon address using the PerpManager contract.
/// Returns detailed results including perp IDs for successful deployments.
#[openapi(tag = "Perpetual")]
#[post("/batch_deploy_perps_for_beacons", data = "<request>")]
pub async fn batch_deploy_perps_for_beacons(
    request: Json<BatchDeployPerpsForBeaconsRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<BatchDeployPerpsForBeaconsResponse>>, Status> {
    tracing::info!("Received request: POST /batch_deploy_perps_for_beacons");
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/batch_deploy_perps_for_beacons");
        scope.set_extra("requested_count", request.beacon_addresses.len().into());
    });

    let beacon_count = request.beacon_addresses.len();

    // Validate the count (similar to batch beacon creation)
    if beacon_count == 0 || beacon_count > 10 {
        tracing::warn!("Invalid beacon count: {}", beacon_count);
        return Err(Status::BadRequest);
    }

    // Parse module addresses (shared across all perps in the batch)
    let fees_module = match Address::from_str(&request.fees_module) {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("Invalid fees module address: {}", e);
            return Err(Status::BadRequest);
        }
    };

    let margin_ratios_module = match Address::from_str(&request.margin_ratios_module) {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("Invalid margin ratios module address: {}", e);
            return Err(Status::BadRequest);
        }
    };

    let lockup_period_module = match Address::from_str(&request.lockup_period_module) {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("Invalid lockup period module address: {}", e);
            return Err(Status::BadRequest);
        }
    };

    let sqrt_price_impact_limit_module =
        match Address::from_str(&request.sqrt_price_impact_limit_module) {
            Ok(addr) => addr,
            Err(e) => {
                tracing::error!("Invalid sqrt price impact limit module address: {}", e);
                return Err(Status::BadRequest);
            }
        };

    let starting_sqrt_price_x96 = match U160::from_str(&request.starting_sqrt_price_x96) {
        Ok(price) => price,
        Err(e) => {
            tracing::error!("Invalid starting sqrt price X96: {}", e);
            return Err(Status::BadRequest);
        }
    };

    let mut perp_ids = Vec::new();
    let mut errors = Vec::new();

    for (i, beacon_address) in request.beacon_addresses.iter().enumerate() {
        let index = i + 1;
        tracing::info!(
            "Deploying perp {}/{} for beacon {}",
            index,
            beacon_count,
            beacon_address
        );

        // Parse the beacon address
        let beacon_addr = match Address::from_str(beacon_address) {
            Ok(addr) => addr,
            Err(e) => {
                let error_msg =
                    format!("Failed to parse beacon address {index} ({beacon_address}): {e}");
                tracing::error!("{}", error_msg);
                errors.push(error_msg.clone());
                sentry::capture_message(&error_msg, sentry::Level::Error);
                continue;
            }
        };

        match deploy_perp_for_beacon(
            state,
            beacon_addr,
            fees_module,
            margin_ratios_module,
            lockup_period_module,
            sqrt_price_impact_limit_module,
            starting_sqrt_price_x96,
        )
        .await
        {
            Ok(response) => {
                let perp_id = response.perp_id.clone();
                perp_ids.push(response.perp_id);
                tracing::info!(
                    "Successfully deployed perp {}: {} for beacon {}",
                    index,
                    perp_id,
                    beacon_address
                );
            }
            Err(e) => {
                let error_msg =
                    format!("Failed to deploy perp {index} for beacon {beacon_address}: {e}");
                tracing::error!("{}", error_msg);
                errors.push(error_msg.clone());
                sentry::capture_message(&error_msg, sentry::Level::Error);
                continue; // Continue with next beacon instead of failing entire batch
            }
        }
    }

    let deployed_count = perp_ids.len() as u32;
    let failed_count = beacon_count as u32 - deployed_count;

    let response_data = BatchDeployPerpsForBeaconsResponse {
        deployed_count,
        perp_ids: perp_ids.clone(),
        failed_count,
        errors,
    };

    let message = if failed_count == 0 {
        format!("Successfully deployed perps for all {deployed_count} beacons")
    } else if deployed_count == 0 {
        "Failed to deploy any perps".to_string()
    } else {
        format!("Partially successful: {deployed_count} deployed, {failed_count} failed")
    };

    tracing::info!("{}", message);

    // Return success even with partial failures, let client handle the response
    Ok(Json(ApiResponse {
        success: deployed_count > 0,
        data: Some(response_data),
        message,
    }))
}

// Tests moved to tests/unit_tests/perp_route_tests.rs
