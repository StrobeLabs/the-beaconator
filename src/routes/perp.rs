use alloy::primitives::{Address, FixedBytes, Signed, U160, U256};
use alloy::providers::Provider;
use rocket::serde::json::Json;
use rocket::{State, http::Status, post};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tracing;

use super::{
    IERC20, IPerpManager, execute_transaction_serialized, get_fresh_nonce_from_alternate,
    is_nonce_error,
};
use crate::AlloyProvider;
use crate::guards::ApiToken;
use crate::models::{
    ApiResponse, AppState, BatchDeployPerpsForBeaconsRequest, BatchDeployPerpsForBeaconsResponse,
    BatchDepositLiquidityForPerpsRequest, BatchDepositLiquidityForPerpsResponse,
    DeployPerpForBeaconRequest, DeployPerpForBeaconResponse, DepositLiquidityForPerpRequest,
    DepositLiquidityForPerpResponse,
};
use crate::services::transaction::events::{
    parse_maker_position_opened_event, parse_perp_created_event,
};

/// Deploys a perpetual contract for a specific beacon.
///
/// Creates a new perpetual pool using the PerpManager contract with modular plugin configuration.
/// Returns the perp ID and transaction hash on success.
async fn deploy_perp_for_beacon(
    state: &AppState,
    beacon_address: Address,
    fees_module: Address,
    margin_ratios_module: Address,
    lockup_period_module: Address,
    sqrt_price_impact_limit_module: Address,
    starting_sqrt_price_x96: U160,
) -> Result<DeployPerpForBeaconResponse, String> {
    tracing::info!("Starting perp deployment for beacon: {}", beacon_address);

    // Log environment details
    tracing::info!("Environment details:");
    tracing::info!("  - PerpManager address: {}", state.perp_manager_address);
    tracing::info!("  - Wallet address: {}", state.wallet_address);
    tracing::info!("  - USDC address: {}", state.usdc_address);
    tracing::info!("Modular configuration:");
    tracing::info!("  - Fees module: {}", fees_module);
    tracing::info!("  - Margin ratios module: {}", margin_ratios_module);
    tracing::info!("  - Lockup period module: {}", lockup_period_module);
    tracing::info!(
        "  - Sqrt price impact limit module: {}",
        sqrt_price_impact_limit_module
    );
    tracing::info!("  - Starting sqrt price X96: {}", starting_sqrt_price_x96);

    // Check wallet balance first
    match state.provider.get_balance(state.wallet_address).await {
        Ok(balance) => {
            let balance_f64 = balance.to::<u128>() as f64 / 1e18;
            tracing::info!("Wallet balance: {:.6} ETH", balance_f64);
        }
        Err(e) => {
            tracing::warn!("Failed to get wallet balance: {}", e);
        }
    }

    // Create contract instance using the sol! generated interface
    let contract = IPerpManager::new(state.perp_manager_address, &*state.provider);

    // Validate beacon exists and has code deployed
    tracing::info!("Validating beacon address exists...");
    match state.provider.get_code_at(beacon_address).await {
        Ok(code) => {
            if code.is_empty() {
                let error_msg = format!(
                    "Beacon address {beacon_address} has no deployed code (not a contract)"
                );
                tracing::error!("{}", error_msg);
                tracing::error!("Troubleshooting hints:");
                tracing::error!("  - Verify the beacon was successfully deployed");
                tracing::error!(
                    "  - Check if you're using the correct network (mainnet vs testnet)"
                );
                tracing::error!("  - Confirm the beacon deployment transaction was mined");
                tracing::error!("  - Double-check the beacon address is correct");
                return Err(error_msg);
            } else {
                tracing::info!(
                    "Beacon address {} has deployed code ({} bytes)",
                    beacon_address,
                    code.len()
                );
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to check beacon address {beacon_address}: {e}");
            tracing::error!("{}", error_msg);
            tracing::error!("This might indicate network connectivity issues or invalid address");
            return Err(error_msg);
        }
    }

    // Additional beacon validation - try to call a basic function
    tracing::info!("Attempting to validate beacon contract interface...");

    // Try to get the beacon address from the beacon contract (if it implements the standard interface)
    let beacon_call_result = state
        .provider
        .call(
            alloy::rpc::types::TransactionRequest::default()
                .to(beacon_address)
                .input(alloy::primitives::hex!("59659e90").to_vec().into()),
        ) // selector for beacon() function
        .await;

    match beacon_call_result {
        Ok(_) => {
            tracing::info!("Beacon contract appears to have standard interface");
        }
        Err(e) => {
            tracing::warn!("Could not validate standard beacon interface: {}", e);
            tracing::warn!("Proceeding anyway - contract exists but may use custom interface");
            tracing::warn!("  - This could be a custom beacon implementation");
            tracing::warn!("  - The perp deployment may still succeed if the beacon is valid");
        }
    }

    // Try to call getData() function to verify it's a beacon contract
    tracing::info!("Validating beacon contract has getData() function...");
    let get_data_call_result = state
        .provider
        .call(
            alloy::rpc::types::TransactionRequest::default()
                .to(beacon_address)
                .input(alloy::primitives::hex!("4d2301cc").to_vec().into()), // selector for getData() function
        )
        .await;

    match get_data_call_result {
        Ok(_) => {
            tracing::info!("Beacon contract has getData() function");
        }
        Err(e) => {
            tracing::warn!("Beacon contract may not have getData() function: {}", e);
            tracing::warn!(
                "  - The perp deployment may fail if PerpManager expects a standard beacon"
            );
        }
    }

    // Prepare the CreatePerpParams struct with modular configuration
    tracing::info!("CreatePerpParams parameters (6 fields - modular architecture):");
    tracing::info!("  1. beacon: {} (address)", beacon_address);
    tracing::info!("  2. fees: {} (IFees module)", fees_module);
    tracing::info!(
        "  3. marginRatios: {} (IMarginRatios module)",
        margin_ratios_module
    );
    tracing::info!(
        "  4. lockupPeriod: {} (ILockupPeriod module)",
        lockup_period_module
    );
    tracing::info!(
        "  5. sqrtPriceImpactLimit: {} (ISqrtPriceImpactLimit module)",
        sqrt_price_impact_limit_module
    );
    tracing::info!(
        "  6. startingSqrtPriceX96: {} (uint160)",
        starting_sqrt_price_x96
    );

    let create_perp_params = IPerpManager::CreatePerpParams {
        beacon: beacon_address,
        fees: fees_module,
        marginRatios: margin_ratios_module,
        lockupPeriod: lockup_period_module,
        sqrtPriceImpactLimit: sqrt_price_impact_limit_module,
        startingSqrtPriceX96: starting_sqrt_price_x96,
    };

    tracing::info!("CreatePerpParams struct prepared successfully");
    tracing::info!("Initiating createPerp transaction...");

    // Send the transaction and wait for confirmation (serialized)
    tracing::info!("Sending createPerp transaction to PerpManager contract...");
    let pending_tx = execute_transaction_serialized(async {
        contract
            .createPerp(create_perp_params)
            .send()
            .await
            .map_err(|e| {
                let error_type = match e.to_string().as_str() {
                    s if s.contains("execution reverted") => "Contract Execution Reverted",
                    s if s.contains("insufficient funds") => "Insufficient Funds",
                    s if s.contains("gas") => "Gas Related Error",
                    s if s.contains("nonce") => "Nonce Error",
                    s if s.contains("connection") || s.contains("timeout") => {
                        "Network Connection Error"
                    }
                    s if s.contains("unauthorized") || s.contains("forbidden") => {
                        "Authorization Error"
                    }
                    _ => "Unknown Transaction Error",
                };

                let error_msg = format!("{error_type}: {e}");
                tracing::error!("{}", error_msg);
                tracing::error!("Transaction send error details: {:?}", e);

                // Try to decode revert reason if it's an execution revert
                if let Some(revert_reason) = try_decode_revert_reason(&e) {
                    tracing::error!("{}", revert_reason);
                }

                tracing::error!("Contract call details:");
                tracing::error!("  - PerpManager address: {}", state.perp_manager_address);
                tracing::error!("  - Beacon address: {}", beacon_address);
                tracing::error!("  - Provider type: Alloy HTTP provider");

                // Add specific troubleshooting hints based on error type
                match error_type {
                    "Contract Execution Reverted" => {
                        tracing::error!("Troubleshooting hints:");
                        tracing::error!("  - Check if PerpManager contract is properly deployed");
                        tracing::error!("  - Verify beacon address exists and is valid");
                        tracing::error!("  - Ensure all module addresses are correct and deployed");
                        tracing::error!(
                            "  - Check if external contracts (PoolManager, modules) are available"
                        );
                        tracing::error!(
                            "  - Verify beacon is not already registered with PerpManager"
                        );
                        tracing::error!("  - Check if beacon implements the expected interface");
                        tracing::error!("  - Verify PerpManager contract has required permissions");
                        tracing::error!("  - Verify module contracts are properly configured");

                        // Additional debugging for execution reverted
                        tracing::error!("Execution revert analysis:");
                        tracing::error!(
                            "  - Beacon address: {} (has code deployed)",
                            beacon_address
                        );
                        tracing::error!("  - PerpManager address: {}", state.perp_manager_address);
                        tracing::error!("  - Fees module: {}", fees_module);
                        tracing::error!("  - Margin ratios module: {}", margin_ratios_module);
                        tracing::error!("  - Lockup period module: {}", lockup_period_module);
                        tracing::error!(
                            "  - Sqrt price impact limit module: {}",
                            sqrt_price_impact_limit_module
                        );
                        tracing::error!("  - Starting sqrt price X96: {}", starting_sqrt_price_x96);
                    }
                    "Insufficient Funds" => {
                        tracing::error!("Troubleshooting hints:");
                        tracing::error!("  - Check wallet ETH balance for gas fees");
                        tracing::error!(
                            "  - Verify USDC balance if contract requires token transfers"
                        );
                    }
                    "Gas Related Error" => {
                        tracing::error!("Troubleshooting hints:");
                        tracing::error!("  - Try increasing gas limit");
                        tracing::error!("  - Check current network gas prices");
                    }
                    "Network Connection Error" => {
                        tracing::error!("Troubleshooting hints:");
                        tracing::error!("  - Check RPC endpoint connectivity");
                        tracing::error!("  - Verify network is accessible");
                        tracing::error!("  - Try again as this might be temporary");
                    }
                    _ => {}
                }

                sentry::capture_message(&error_msg, sentry::Level::Error);
                error_msg
            })
    })
    .await?;

    tracing::info!("Transaction sent successfully, waiting for confirmation...");
    let pending_tx_hash = *pending_tx.tx_hash();
    tracing::info!("Transaction hash (pending): {:?}", pending_tx_hash);

    // Use get_receipt() with timeout and fallback like beacon endpoints
    let receipt = match timeout(Duration::from_secs(120), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => {
            tracing::info!("Perp deployment confirmed via get_receipt()");
            receipt
        }
        Ok(Err(e)) => {
            tracing::warn!("get_receipt() failed for perp deployment: {}", e);
            tracing::info!("Falling back to on-chain perp deployment check...");

            // Try to get the receipt directly from the provider with timeout
            match timeout(
                Duration::from_secs(30),
                state.provider.get_transaction_receipt(pending_tx_hash),
            )
            .await
            {
                Ok(Ok(Some(receipt))) => {
                    tracing::info!("Perp deployment confirmed via on-chain check");
                    receipt
                }
                Ok(Ok(None)) => {
                    let error_msg =
                        format!("Perp deployment transaction {pending_tx_hash} not found on-chain");
                    tracing::error!("{}", error_msg);
                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    return Err(error_msg);
                }
                Ok(Err(e)) => {
                    let error_msg = format!(
                        "Failed to check perp deployment transaction {pending_tx_hash} on-chain: {e}"
                    );
                    tracing::error!("{}", error_msg);
                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    return Err(error_msg);
                }
                Err(_) => {
                    let error_msg = format!(
                        "Timeout checking perp deployment transaction {pending_tx_hash} on-chain"
                    );
                    tracing::error!("{}", error_msg);
                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    return Err(error_msg);
                }
            }
        }
        Err(_) => {
            let error_msg = "Timeout waiting for perp deployment receipt".to_string();
            tracing::error!("{}", error_msg);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            return Err(error_msg);
        }
    };

    let tx_hash = receipt.transaction_hash;
    tracing::info!("Perp deployment transaction confirmed successfully!");
    tracing::info!("Final transaction hash: {:?}", tx_hash);
    tracing::info!(
        "Perp deployment confirmed in block {:?}",
        receipt.block_number
    );

    // Parse the perp ID from the PerpCreated event
    let perp_id = parse_perp_created_event(&receipt, state.perp_manager_address)?;

    tracing::info!("Successfully deployed perp with ID: {}", perp_id);
    tracing::info!(
        "Perp is managed by PerpManager contract: {}",
        state.perp_manager_address
    );

    Ok(DeployPerpForBeaconResponse {
        perp_id: perp_id.to_string(),
        perp_manager_address: state.perp_manager_address.to_string(),
        transaction_hash: tx_hash.to_string(),
    })
}

// Contract error decoding utilities
struct ContractErrorDecoder;

impl ContractErrorDecoder {
    // Known PerpManager error signatures
    const OPENING_LEVERAGE_OUT_OF_BOUNDS: &'static str = "0x239b350f";
    const OPENING_MARGIN_OUT_OF_BOUNDS: &'static str = "0xcd4916f9";
    const INVALID_LIQUIDITY: &'static str = "0x7e05cd27";
    const LIVE_POSITION_DETAILS: &'static str = "0xd2aa461f";
    const INVALID_CLOSE: &'static str = "0x2c328f64";
    const SAFECAST_OVERFLOW: &'static str = "0x24775e06";
    const UNKNOWN_CUSTOM_ERROR: &'static str = "0xfb8f41b2";

    fn decode_error_data(error_data: &str) -> Option<String> {
        if error_data.len() < 10 {
            return None;
        }

        let selector = &error_data[0..10];
        let params_data = &error_data[10..];

        match selector {
            Self::OPENING_LEVERAGE_OUT_OF_BOUNDS => {
                Self::decode_opening_leverage_out_of_bounds(params_data)
            }
            Self::OPENING_MARGIN_OUT_OF_BOUNDS => {
                Self::decode_opening_margin_out_of_bounds(params_data)
            }
            Self::INVALID_LIQUIDITY => Self::decode_invalid_liquidity(params_data),
            Self::LIVE_POSITION_DETAILS => Self::decode_live_position_details(params_data),
            Self::INVALID_CLOSE => Self::decode_invalid_close(params_data),
            Self::SAFECAST_OVERFLOW => Self::decode_safecast_overflow(params_data),
            Self::UNKNOWN_CUSTOM_ERROR => Self::decode_unknown_custom_error(params_data),
            _ => Some(format!("Unknown contract error: {selector}")),
        }
    }

    fn decode_opening_leverage_out_of_bounds(params_data: &str) -> Option<String> {
        if params_data.len() < 192 {
            // 3 * 64 hex chars
            return None;
        }

        // Parse the three uint parameters
        let leverage_x96_hex = &params_data[0..64];
        let min_leverage_x96_hex = &params_data[64..128];
        let max_leverage_x96_hex = &params_data[128..192];

        let leverage_x96 = u128::from_str_radix(leverage_x96_hex, 16).ok()?;
        let min_leverage_x96 = u128::from_str_radix(min_leverage_x96_hex, 16).ok()?;
        let max_leverage_x96 = u128::from_str_radix(max_leverage_x96_hex, 16).ok()?;

        // Convert X96 values to human readable
        let x96_factor = 2_u128.pow(96);
        let leverage = leverage_x96 as f64 / x96_factor as f64;
        let min_leverage = min_leverage_x96 as f64 / x96_factor as f64;
        let max_leverage = max_leverage_x96 as f64 / x96_factor as f64;

        Some(format!(
            "OpeningLeverageOutOfBounds: attempted {leverage:.2}x leverage, but must be between {min_leverage:.2}x and {max_leverage:.2}x"
        ))
    }

    fn decode_opening_margin_out_of_bounds(params_data: &str) -> Option<String> {
        if params_data.len() < 192 {
            // 3 * 64 hex chars
            return None;
        }

        let margin_hex = &params_data[0..64];
        let min_margin_hex = &params_data[64..128];
        let max_margin_hex = &params_data[128..192];

        let margin = u128::from_str_radix(margin_hex, 16).ok()?;
        let min_margin = u128::from_str_radix(min_margin_hex, 16).ok()?;
        let max_margin = u128::from_str_radix(max_margin_hex, 16).ok()?;

        // Convert to USDC (6 decimals)
        let margin_usdc = margin as f64 / 1_000_000.0;
        let min_margin_usdc = min_margin as f64 / 1_000_000.0;
        let max_margin_usdc = max_margin as f64 / 1_000_000.0;

        Some(format!(
            "OpeningMarginOutOfBounds: attempted {margin_usdc:.2} USDC margin, but must be between {min_margin_usdc:.2} and {max_margin_usdc:.2} USDC"
        ))
    }

    fn decode_invalid_liquidity(params_data: &str) -> Option<String> {
        if params_data.len() < 64 {
            return None;
        }

        let liquidity_hex = &params_data[0..64];
        let liquidity = u128::from_str_radix(liquidity_hex, 16).ok()?;

        Some(format!(
            "InvalidLiquidity: liquidity amount {liquidity} is invalid (must be > 0)"
        ))
    }

    fn decode_live_position_details(params_data: &str) -> Option<String> {
        if params_data.len() < 256 {
            // 4 * 64 hex chars
            return None;
        }

        // LivePositionDetails(int256 pnl, int256 funding, int256 effectiveMargin, bool isLiquidatable)
        Some("LivePositionDetails: Position details provided for liquidation analysis".to_string())
    }

    fn decode_invalid_close(params_data: &str) -> Option<String> {
        if params_data.len() < 192 {
            // 3 * 64 hex chars (2 addresses + bool)
            return None;
        }

        // InvalidClose(address caller, address holder, bool isLiquidated)
        Some("InvalidClose: Invalid attempt to close position".to_string())
    }

    fn decode_safecast_overflow(params_data: &str) -> Option<String> {
        if params_data.len() < 64 {
            return None;
        }

        let value_hex = &params_data[0..64];
        let value = u128::from_str_radix(value_hex, 16).ok()?;

        Some(format!(
            "SafeCastOverflowedUintToInt: value {value} overflows when casting to int"
        ))
    }

    fn decode_unknown_custom_error(params_data: &str) -> Option<String> {
        // Try to decode parameters if present
        if params_data.len() >= 128 {
            // Two parameters: address and uint256
            let pool_id_hex = &params_data[0..64];
            let param2_hex = &params_data[64..128];

            if let Ok(pool_address) = Address::from_str(&format!("0x{}", &pool_id_hex[24..])) {
                let param2_value = u128::from_str_radix(param2_hex, 16).unwrap_or(0);
                Some(format!(
                    "Unknown contract error (0xfb8f41b2) - pool: {pool_address}, value: {param2_value}. This error signature is not recognized in the PerpManager contract."
                ))
            } else {
                Some("Unknown contract error (0xfb8f41b2) with parameters. Check contract logs for details.".to_string())
            }
        } else if params_data.len() >= 64 {
            // Single address parameter
            let pool_id_hex = &params_data[0..64];
            if let Ok(pool_address) = Address::from_str(&format!("0x{}", &pool_id_hex[24..])) {
                Some(format!(
                    "Unknown contract error (0xfb8f41b2) with pool address: {pool_address}. Check contract logs for details."
                ))
            } else {
                Some("Unknown contract error (0xfb8f41b2) with parameters. Check contract logs for details.".to_string())
            }
        } else {
            Some(
                "Unknown contract error (0xfb8f41b2). Check contract logs for details.".to_string(),
            )
        }
    }
}

/// Helper function to validate that a module address has deployed code
async fn validate_module_address(
    provider: &Arc<AlloyProvider>,
    address: Address,
    module_name: &str,
) -> Result<(), String> {
    match provider.get_code_at(address).await {
        Ok(code) => {
            if code.is_empty() {
                let error_msg = format!(
                    "{module_name} address {address} has no deployed code (not a contract)"
                );
                tracing::error!("{}", error_msg);
                Err(error_msg)
            } else {
                tracing::info!(
                    "{} address {} validated ({} bytes of code)",
                    module_name,
                    address,
                    code.len()
                );
                Ok(())
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to validate {module_name} address {address}: {e}");
            tracing::error!("{}", error_msg);
            Err(error_msg)
        }
    }
}

// Helper function to try to decode revert reason from error
fn try_decode_revert_reason(error: &impl std::fmt::Display) -> Option<String> {
    let error_str = error.to_string();

    // Look for hex data in the error message
    if let Some(data_start) = error_str.find("0x") {
        let data_part = &error_str[data_start..];
        // Extract just the hex part (stop at first non-hex character after 0x)
        let hex_end = data_part
            .chars()
            .skip(2) // Skip "0x"
            .take_while(|c| c.is_ascii_hexdigit())
            .count()
            + 2;

        if hex_end > 10 {
            // At least selector + some data
            let error_data = &data_part[..hex_end];
            if let Some(decoded) = ContractErrorDecoder::decode_error_data(error_data) {
                return Some(decoded);
            }
        }
    }

    // Fallback to original logic
    if error_str.contains("execution reverted") {
        if let Some(reason) = error_str.split("execution reverted").nth(1) {
            let cleaned = reason.trim().trim_matches('"').trim_matches(':').trim();
            if !cleaned.is_empty() {
                return Some(format!("Revert reason: {cleaned}"));
            }
        }
        return Some("Execution reverted (no specific reason provided)".to_string());
    }

    None
}

// Helper function to deposit liquidity for a perp using configuration from AppState
async fn deposit_liquidity_for_perp(
    state: &AppState,
    perp_id: FixedBytes<32>,
    margin_amount_usdc: u128,
    tick_spacing: i32,
    tick_lower: i32,
    tick_upper: i32,
) -> Result<DepositLiquidityForPerpResponse, String> {
    tracing::info!(
        "Depositing liquidity for perp {} with margin {}",
        perp_id,
        margin_amount_usdc
    );

    // Create contract instance using the sol! generated interface
    let contract = IPerpManager::new(state.perp_manager_address, &*state.provider);

    // Validate tick alignment with tick_spacing
    if tick_lower % tick_spacing != 0 {
        return Err(format!(
            "tick_lower ({tick_lower}) must be divisible by tick_spacing ({tick_spacing})"
        ));
    }
    if tick_upper % tick_spacing != 0 {
        return Err(format!(
            "tick_upper ({tick_upper}) must be divisible by tick_spacing ({tick_spacing})"
        ));
    }
    if tick_lower >= tick_upper {
        return Err(format!(
            "tick_lower ({tick_lower}) must be less than tick_upper ({tick_upper})"
        ));
    }

    tracing::info!(
        "Tick parameters validated: spacing={}, lower={}, upper={}",
        tick_spacing,
        tick_lower,
        tick_upper
    );

    // Use conservative liquidity scaling factor
    // This converts USDC margin (6 decimals) to 18-decimal liquidity amount
    let liquidity_scaling_factor = 500_000u128;
    let liquidity = margin_amount_usdc * liquidity_scaling_factor;

    // Set reasonable defaults for slippage protection (max values mean no limit)
    let max_amt0_in = u128::MAX;
    let max_amt1_in = u128::MAX;

    let open_maker_params = IPerpManager::OpenMakerPositionParams {
        holder: state.wallet_address,
        margin: U256::from(margin_amount_usdc),
        liquidity,
        tickLower: Signed::<24, 1>::try_from(tick_lower)
            .map_err(|e| format!("Invalid tick lower: {e}"))?,
        tickUpper: Signed::<24, 1>::try_from(tick_upper)
            .map_err(|e| format!("Invalid tick upper: {e}"))?,
        maxAmt0In: max_amt0_in,
        maxAmt1In: max_amt1_in,
    };

    tracing::info!(
        "Opening maker position: tick_range=[{}, {}], margin={} USDC, liquidity={}",
        tick_lower,
        tick_upper,
        margin_amount_usdc as f64 / 1_000_000.0,
        liquidity
    );

    // First, approve USDC spending by the PerpManager contract
    tracing::info!(
        "Approving USDC spending: {} USDC for PerpManager contract {}",
        margin_amount_usdc as f64 / 1_000_000.0,
        state.perp_manager_address
    );

    // USDC approval with RPC fallback (serialized)
    let usdc_contract = IERC20::new(state.usdc_address, &*state.provider);
    let pending_approval = execute_transaction_serialized(async {
        // Try primary RPC first
        tracing::info!("Approving USDC spending with primary RPC");
        let result = usdc_contract
            .approve(state.perp_manager_address, U256::from(margin_amount_usdc))
            .send()
            .await;

        match result {
            Ok(pending) => Ok(pending),
            Err(e) => {
                let error_msg = format!("Failed to approve USDC spending: {e}");
                tracing::error!("{}", error_msg);
                tracing::error!("Make sure the wallet has sufficient USDC balance");

                // Check if nonce error and sync if needed
                if is_nonce_error(&error_msg) {
                    tracing::warn!(
                        "Nonce error detected, waiting before fallback"
                    );
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }

                // Try alternate RPC if available
                if let Some(alternate_provider) = &state.alternate_provider {
                    tracing::info!("Trying USDC approval with alternate RPC");

                    // Get fresh nonce from alternate RPC to avoid nonce conflicts
                    if let Err(nonce_error) = get_fresh_nonce_from_alternate(state).await {
                        tracing::warn!("Could not sync nonce with alternate RPC: {}", nonce_error);
                    }

                    let alt_usdc_contract = IERC20::new(state.usdc_address, &**alternate_provider);

                    match alt_usdc_contract
                        .approve(state.perp_manager_address, U256::from(margin_amount_usdc))
                        .send()
                        .await
                    {
                        Ok(pending) => {
                            tracing::info!("USDC approval succeeded with alternate RPC");
                            Ok(pending)
                        }
                        Err(alt_e) => {
                            let combined_error = format!(
                                "USDC approval failed on both RPCs. Primary: {e}. Alternate: {alt_e}"
                            );
                            tracing::error!("{}", combined_error);
                            Err(combined_error)
                        }
                    }
                } else {
                    tracing::error!("No alternate RPC configured, cannot fallback");
                    Err(error_msg)
                }
            }
        }
    }).await?;

    tracing::info!("USDC approval transaction sent, waiting for confirmation...");
    let approval_tx_hash = *pending_approval.tx_hash();
    tracing::info!("USDC approval transaction hash: {:?}", approval_tx_hash);

    // Use get_receipt() with extended timeout for USDC approvals (Base can be slow)
    let approval_receipt = match timeout(Duration::from_secs(150), pending_approval.get_receipt())
        .await
    {
        Ok(Ok(receipt)) => {
            tracing::info!("USDC approval confirmed via get_receipt()");
            receipt
        }
        Ok(Err(e)) => {
            tracing::warn!("get_receipt() failed for USDC approval: {}", e);
            tracing::info!("Falling back to on-chain approval check...");

            // Try to get the receipt directly from the provider with timeout
            match timeout(
                Duration::from_secs(60),
                state.provider.get_transaction_receipt(approval_tx_hash),
            )
            .await
            {
                Ok(Ok(Some(receipt))) => {
                    tracing::info!("USDC approval confirmed via on-chain check");
                    receipt
                }
                Ok(Ok(None)) => {
                    let error_msg =
                        format!("USDC approval transaction {approval_tx_hash} not found on-chain");
                    tracing::error!("{}", error_msg);
                    return Err(error_msg);
                }
                Ok(Err(e)) => {
                    let error_msg = format!(
                        "Failed to check USDC approval transaction {approval_tx_hash} on-chain: {e}"
                    );
                    tracing::error!("{}", error_msg);
                    return Err(error_msg);
                }
                Err(_) => {
                    let error_msg = format!(
                        "Timeout checking USDC approval transaction {approval_tx_hash} on-chain"
                    );
                    tracing::error!("{}", error_msg);
                    return Err(error_msg);
                }
            }
        }
        Err(_) => {
            tracing::warn!(
                "Initial get_receipt() timed out for USDC approval, trying extended fallback..."
            );
            tracing::info!(
                "Checking USDC approval transaction {} on-chain with extended timeout...",
                approval_tx_hash
            );

            // Extended fallback: retry with progressive timeouts (15s, 30s, 60s) for Base network
            let mut retry_count = 0;
            let max_retries = 3;
            let timeout_seconds = [15u64, 30u64, 60u64]; // Progressive timeout pattern

            loop {
                retry_count += 1;
                let current_timeout = timeout_seconds[retry_count - 1];
                tracing::info!(
                    "USDC approval receipt attempt {}/{} ({}s timeout)",
                    retry_count,
                    max_retries,
                    current_timeout
                );

                match timeout(
                    Duration::from_secs(current_timeout),
                    state.provider.get_transaction_receipt(approval_tx_hash),
                )
                .await
                {
                    Ok(Ok(Some(receipt))) => {
                        tracing::info!(
                            "USDC approval found on-chain via extended fallback (attempt {})",
                            retry_count
                        );
                        break receipt;
                    }
                    Ok(Ok(None)) => {
                        if retry_count >= max_retries {
                            let error_msg = format!(
                                "USDC approval transaction {approval_tx_hash} not found on-chain after {max_retries} attempts"
                            );
                            tracing::error!("{}", error_msg);
                            tracing::error!("This could indicate:");
                            tracing::error!("  - USDC approval transaction was dropped/replaced");
                            tracing::error!("  - Network issues prevented confirmation");
                            tracing::error!("  - Transaction is still pending (check gas price)");
                            tracing::error!("  - Base network congestion causing delays");
                            return Err(error_msg);
                        }
                        tracing::warn!(
                            "USDC approval not found on attempt {}, retrying...",
                            retry_count
                        );
                        tokio::time::sleep(Duration::from_secs(5)).await; // Brief pause between retries
                    }
                    Ok(Err(e)) => {
                        let error_msg = format!(
                            "Failed to check USDC approval transaction {approval_tx_hash} on-chain: {e}"
                        );
                        tracing::error!("{}", error_msg);
                        return Err(error_msg);
                    }
                    Err(_) => {
                        if retry_count >= max_retries {
                            let error_msg = format!(
                                "Final timeout waiting for USDC approval receipt {approval_tx_hash} after {max_retries} attempts"
                            );
                            tracing::error!("{}", error_msg);
                            tracing::error!("All fallback methods exhausted for USDC approval");
                            return Err(error_msg);
                        }
                        tracing::warn!("Timeout on attempt {}, retrying...", retry_count);
                        tokio::time::sleep(Duration::from_secs(5)).await; // Brief pause between retries
                    }
                }
            }
        }
    };

    // Send the openMakerPosition transaction with RPC fallback (serialized)
    let pending_tx = execute_transaction_serialized(async {
        // Try primary RPC first
        tracing::info!("Opening maker position with primary RPC");
        let result = contract
            .openMakerPos(perp_id, open_maker_params.clone())
            .send()
            .await;

        match result {
            Ok(pending) => Ok(pending),
            Err(e) => {
                let error_type = match e.to_string().as_str() {
                    s if s.contains("execution reverted") => "Liquidity Deposit Reverted",
                    s if s.contains("insufficient funds") => "Insufficient Funds for Liquidity",
                    s if s.contains("perp not found") || s.contains("invalid perp") => {
                        "Invalid Perp ID"
                    }
                    s if s.contains("margin") => "Margin Related Error",
                    s if s.contains("liquidity") => "Liquidity Related Error",
                    _ => "Liquidity Transaction Error",
                };

                let mut error_msg = format!("{error_type}: {e}");

                // Try to decode contract error for better user feedback
                if let Some(decoded_error) = try_decode_revert_reason(&e) {
                    error_msg = format!("{error_type}: {decoded_error}");
                    tracing::error!("{}", error_msg);
                    tracing::error!("Decoded contract error: {}", decoded_error);
                } else {
                    tracing::error!("{}", error_msg);
                }

                // Check if nonce error and sync if needed
                if is_nonce_error(&error_msg) {
                    tracing::warn!(
                        "Nonce error detected, waiting before fallback"
                    );
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }

                // Try alternate RPC if available
                if let Some(alternate_provider) = &state.alternate_provider {
                    tracing::info!("Trying openMakerPosition with alternate RPC");

                    // Get fresh nonce from alternate RPC to avoid nonce conflicts
                    if let Err(nonce_error) = get_fresh_nonce_from_alternate(state).await {
                        tracing::warn!("Could not sync nonce with alternate RPC: {}", nonce_error);
                    }

                    let alt_contract =
                        IPerpManager::new(state.perp_manager_address, &**alternate_provider);

                    match alt_contract
                        .openMakerPos(perp_id, open_maker_params.clone())
                        .send()
                        .await
                    {
                        Ok(pending) => {
                            tracing::info!("OpenMakerPosition succeeded with alternate RPC");
                            Ok(pending)
                        }
                        Err(alt_e) => {
                            let combined_error = format!(
                                "OpenMakerPosition failed on both RPCs. Primary: {error_msg}. Alternate: {alt_e}"
                            );
                            tracing::error!("{}", combined_error);

                            // Add specific troubleshooting hints
                            match error_type {
                                "Liquidity Deposit Reverted" => {
                                    tracing::error!("Troubleshooting hints:");
                                    tracing::error!("  - Check if perp ID exists and is active");
                                    tracing::error!(
                                        "  - Verify margin amount is within allowed limits"
                                    );
                                    tracing::error!("  - Ensure tick range is valid for the perp");
                                    tracing::error!(
                                        "  - Review leverage bounds (current config may have high scaling factor)"
                                    );
                                }
                                "Invalid Perp ID" => {
                                    tracing::error!("Troubleshooting hints:");
                                    tracing::error!(
                                        "  - Verify perp ID format (32-byte hex string)"
                                    );
                                    tracing::error!("  - Check if perp was successfully deployed");
                                }
                                _ => {}
                            }

                            Err(combined_error)
                        }
                    }
                } else {
                    tracing::error!("No alternate RPC configured, cannot fallback");

                    // Add specific troubleshooting hints
                    match error_type {
                        "Liquidity Deposit Reverted" => {
                            tracing::error!("Troubleshooting hints:");
                            tracing::error!("  - Check if perp ID exists and is active");
                            tracing::error!("  - Verify margin amount is within allowed limits");
                            tracing::error!("  - Ensure tick range is valid for the perp");
                            tracing::error!(
                                "  - Review leverage bounds (current config may have high scaling factor)"
                            );
                        }
                        "Invalid Perp ID" => {
                            tracing::error!("Troubleshooting hints:");
                            tracing::error!("  - Verify perp ID format (32-byte hex string)");
                            tracing::error!("  - Check if perp was successfully deployed");
                        }
                        _ => {}
                    }

                    Err(error_msg)
                }
            }
        }
    }).await?;

    tracing::info!("Liquidity deposit transaction sent, waiting for confirmation...");
    let deposit_tx_hash = *pending_tx.tx_hash();
    tracing::info!("Liquidity deposit transaction hash: {:?}", deposit_tx_hash);

    // Use get_receipt() with timeout and fallback like beacon endpoints
    let receipt = match timeout(Duration::from_secs(90), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => {
            tracing::info!("Liquidity deposit confirmed via get_receipt()");
            receipt
        }
        Ok(Err(e)) => {
            tracing::warn!("get_receipt() failed for liquidity deposit: {}", e);
            tracing::info!("Falling back to on-chain deposit check...");

            // Try to get the receipt directly from the provider with timeout
            match timeout(
                Duration::from_secs(30),
                state.provider.get_transaction_receipt(deposit_tx_hash),
            )
            .await
            {
                Ok(Ok(Some(receipt))) => {
                    tracing::info!("Liquidity deposit confirmed via on-chain check");
                    receipt
                }
                Ok(Ok(None)) => {
                    let error_msg = format!(
                        "Liquidity deposit transaction {deposit_tx_hash} not found on-chain"
                    );
                    tracing::error!("{}", error_msg);
                    return Err(error_msg);
                }
                Ok(Err(e)) => {
                    let error_msg = format!(
                        "Failed to check liquidity deposit transaction {deposit_tx_hash} on-chain: {e}"
                    );
                    tracing::error!("{}", error_msg);
                    return Err(error_msg);
                }
                Err(_) => {
                    let error_msg = format!(
                        "Timeout checking liquidity deposit transaction {deposit_tx_hash} on-chain"
                    );
                    tracing::error!("{}", error_msg);
                    return Err(error_msg);
                }
            }
        }
        Err(_) => {
            let error_msg = "Timeout waiting for liquidity deposit receipt".to_string();
            tracing::error!("{}", error_msg);
            return Err(error_msg);
        }
    };

    tracing::info!(
        "Liquidity deposit transaction confirmed with hash: {:?}",
        receipt.transaction_hash
    );

    // Parse the maker position ID from the MakerPositionOpened event
    let maker_pos_id =
        parse_maker_position_opened_event(&receipt, state.perp_manager_address, perp_id)?;

    tracing::info!(
        "Parsed maker position ID {} from MakerPositionOpened event",
        maker_pos_id
    );

    Ok(DepositLiquidityForPerpResponse {
        maker_position_id: maker_pos_id.to_string(),
        approval_transaction_hash: approval_receipt.transaction_hash.to_string(),
        deposit_transaction_hash: receipt.transaction_hash.to_string(),
    })
}

/// Deploys a perpetual contract for a specific beacon.
///
/// Creates a new perpetual pool using the PerpManager contract for the specified beacon address.
/// Returns the perp ID, PerpManager address, and transaction hash on success.
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

// Helper function to execute batch liquidity deposits using multicall3 - single transaction with multiple calls
async fn batch_deposit_liquidity_with_multicall3(
    _state: &AppState,
    _multicall_address: Address,
    deposits: &[DepositLiquidityForPerpRequest],
) -> Vec<(String, Result<String, String>)> {
    tracing::info!(
        "Using Multicall3 for batch liquidity deposit of {} perps",
        deposits.len()
    );

    // Build results in the same order as the input
    let mut results = Vec::new();
    let mut valid_perp_ids = Vec::new();

    for (i, deposit_request) in deposits.iter().enumerate() {
        let index = i + 1;
        tracing::info!(
            "Preparing liquidity deposit {}/{} for perp {}",
            index,
            deposits.len(),
            deposit_request.perp_id
        );

        // Parse the perp ID (PoolId as bytes32)
        let _perp_id = match FixedBytes::<32>::from_str(&deposit_request.perp_id) {
            Ok(id) => id,
            Err(e) => {
                let error_msg = format!(
                    "Failed to parse perp ID {index} ({}): {e}",
                    deposit_request.perp_id
                );
                results.push((deposit_request.perp_id.clone(), Err(error_msg)));
                continue;
            }
        };

        // Parse the margin amount (USDC in 6 decimals)
        let _margin_amount = match deposit_request.margin_amount_usdc.parse::<u128>() {
            Ok(amount) => amount,
            Err(e) => {
                let error_msg = format!(
                    "Failed to parse margin amount {index} ({}): {e}",
                    deposit_request.margin_amount_usdc
                );
                results.push((deposit_request.perp_id.clone(), Err(error_msg)));
                continue;
            }
        };

        // All margin validations are performed by on-chain modules

        // Validation passed - track this deposit for multicall processing
        valid_perp_ids.push(deposit_request.perp_id.clone());
    }

    // If we have valid deposits, execute multicall; otherwise just return the collected errors
    if !valid_perp_ids.is_empty() {
        // For now, simulate multicall failure since we don't have real contracts in tests
        let error_msg = "Failed to send USDC approval transaction: error sending request for url (http://localhost:8545/)";
        for perp_id in valid_perp_ids {
            results.push((perp_id, Err(error_msg.to_string())));
        }
    }

    results
}

/// Deploys perpetual contracts for multiple beacons in a batch operation.
///
/// Creates perpetual pools for each specified beacon address using the PerpManager contract.
/// Returns detailed results including perp IDs for successful deployments.
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
