use alloy::primitives::{Address, FixedBytes, U256, Uint};
use alloy::providers::Provider;
use sentry;
use std::str::FromStr;
use std::time::Duration;
use tokio::time::timeout;
use tracing;

use crate::models::{
    AppState, DeployPerpForBeaconResponse, DepositLiquidityForPerpRequest,
    DepositLiquidityForPerpResponse,
};
use crate::routes::{IERC20, IPerpHook, execute_transaction_serialized};
use crate::services::transaction::events::{
    parse_maker_position_opened_event, parse_perp_created_event,
};

use alloy::primitives::Signed;

// Helper function to deploy a perp for a beacon using configuration from AppState
pub async fn deploy_perp_for_beacon(
    state: &AppState,
    beacon_address: Address,
) -> Result<DeployPerpForBeaconResponse, String> {
    tracing::info!("Starting perp deployment for beacon: {}", beacon_address);

    // Log environment details
    tracing::info!("Environment details:");
    tracing::info!("  - PerpHook address: {}", state.perp_hook_address);
    tracing::info!("  - Wallet address: {}", state.wallet_address);
    tracing::info!("  - USDC address: {}", state.usdc_address);

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
    let contract = IPerpHook::new(state.perp_hook_address, &*state.provider);

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
                "  - The perp deployment may fail if PerpHook expects a standard beacon"
            );
        }
    }

    // Additional validation: Check if beacon is already registered with PerpHook
    tracing::info!("Checking if beacon is already registered...");
    let beacon_registration_check = state
        .provider
        .call(
            alloy::rpc::types::TransactionRequest::default()
                .to(state.perp_hook_address)
                .input(alloy::primitives::hex!("8da5cb5b").to_vec().into()), // selector for beacons(address)
        )
        .await;

    match beacon_registration_check {
        Ok(_) => {
            tracing::warn!("Beacon may already be registered with PerpHook");
            tracing::warn!("This could cause the deployment to revert");
        }
        Err(e) => {
            tracing::info!("Beacon appears to be unregistered (or check failed): {}", e);
        }
    }

    // Use configuration from AppState instead of hardcoded values
    let config = &state.perp_config;
    tracing::info!("Perp configuration:");
    tracing::info!(
        "  - Trading fee: {} bps ({}%)",
        config.trading_fee_bps,
        config.trading_fee_bps as f64 / 100.0
    );
    tracing::info!(
        "  - Max margin: {} USDC",
        config.max_margin_usdc as f64 / 1_000_000.0
    );
    tracing::info!("  - Tick spacing: {}", config.tick_spacing);
    tracing::info!(
        "  - Funding interval: {} seconds",
        config.funding_interval_seconds
    );

    let trading_fee = Uint::<24, 1>::from(config.trading_fee_bps);
    let min_margin = config.min_margin_usdc;
    let max_margin = config.max_margin_usdc;
    let min_opening_leverage_x96 = config.min_opening_leverage_x96;
    let max_opening_leverage_x96 = config.max_opening_leverage_x96;
    let liquidation_leverage_x96 = config.liquidation_leverage_x96;
    let liquidation_fee_x96 = config.liquidation_fee_x96;
    let liquidation_fee_split_x96 = config.liquidation_fee_split_x96;
    let funding_interval = config.funding_interval_seconds;
    let tick_spacing = Signed::<24, 1>::try_from(config.tick_spacing).map_err(|e| {
        let error = format!("Invalid tick spacing conversion: {e}");
        tracing::error!("{}", error);
        error
    })?;
    let starting_sqrt_price_x96 = Uint::<160, 3>::from(config.starting_sqrt_price_x96);

    tracing::info!("CreatePerpParams parameters (12 fields total):");
    tracing::info!("  1. beacon: {} (address)", beacon_address);
    tracing::info!("  2. tradingFee: {} (uint24)", trading_fee);
    tracing::info!("  3. minMargin: {} (uint128)", min_margin);
    tracing::info!("  4. maxMargin: {} (uint128)", max_margin);
    tracing::info!(
        "  5. minOpeningLeverageX96: {} (uint128)",
        min_opening_leverage_x96
    );
    tracing::info!(
        "  6. maxOpeningLeverageX96: {} (uint128)",
        max_opening_leverage_x96
    );
    tracing::info!(
        "  7. liquidationLeverageX96: {} (uint128)",
        liquidation_leverage_x96
    );
    tracing::info!("  8. liquidationFeeX96: {} (uint128)", liquidation_fee_x96);
    tracing::info!(
        "  9. liquidationFeeSplitX96: {} (uint128)",
        liquidation_fee_split_x96
    );
    tracing::info!("  10. fundingInterval: {} (int128)", funding_interval);
    tracing::info!("  11. tickSpacing: {} (int24)", tick_spacing);
    tracing::info!(
        "  12. startingSqrtPriceX96: {} (uint160)",
        starting_sqrt_price_x96
    );

    // Verify values match your successful transaction
    tracing::info!("Verifying parameter values:");
    if config.trading_fee_bps == 5000 && min_margin == 0 && max_margin == 1000000000 {
        tracing::info!("  Basic parameters match successful transaction");
    } else {
        tracing::warn!("  Basic parameters don't match expected values!");
        tracing::warn!(
            "    - trading_fee_bps: {} (expected 5000)",
            config.trading_fee_bps
        );
        tracing::warn!("    - min_margin: {} (expected 0)", min_margin);
        tracing::warn!("    - max_margin: {} (expected 1000000000)", max_margin);
    }

    // Note about the missing parameter
    tracing::info!("NOTE: The deployed contract does NOT use tradingFeeCreatorSplitX96");
    tracing::info!("  - The source code includes it, but the deployed version doesn't");
    tracing::info!(
        "  - This is why your manual transaction worked with 12 parameters instead of 13"
    );

    // Prepare the CreatePerpParams struct - matches the DEPLOYED contract (no tradingFeeCreatorSplitX96)
    let create_perp_params = IPerpHook::CreatePerpParams {
        beacon: beacon_address,
        tradingFee: trading_fee,
        minMargin: min_margin,
        maxMargin: max_margin,
        minOpeningLeverageX96: min_opening_leverage_x96,
        maxOpeningLeverageX96: max_opening_leverage_x96,
        liquidationLeverageX96: liquidation_leverage_x96,
        liquidationFeeX96: liquidation_fee_x96,
        liquidationFeeSplitX96: liquidation_fee_split_x96,
        fundingInterval: funding_interval,
        tickSpacing: tick_spacing,
        startingSqrtPriceX96: starting_sqrt_price_x96,
    };

    tracing::info!("CreatePerpParams struct prepared successfully");
    tracing::info!("Initiating createPerp transaction...");

    // Send the transaction and wait for confirmation (serialized)
    tracing::info!("Sending createPerp transaction to PerpHook contract...");
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
                s if s.contains("unauthorized") || s.contains("forbidden") => "Authorization Error",
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
            tracing::error!("  - PerpHook address: {}", state.perp_hook_address);
            tracing::error!("  - Beacon address: {}", beacon_address);
            tracing::error!("  - Provider type: Alloy HTTP provider");

            // Add specific troubleshooting hints based on error type
            match error_type {
                "Contract Execution Reverted" => {
                    tracing::error!("Troubleshooting hints:");
                    tracing::error!("  - Check if PerpHook contract is properly deployed");
                    tracing::error!("  - Verify beacon address exists and is valid");
                    tracing::error!("  - Ensure all constructor parameters are correct");
                    tracing::error!(
                        "  - Check if external contracts (PoolManager, Router, etc.) are available"
                    );
                    tracing::error!("  - Verify beacon is not already registered with PerpHook");
                    tracing::error!("  - Check if beacon implements the expected interface");
                    tracing::error!("  - Verify PerpHook contract has required permissions");

                    // Additional debugging for execution reverted
                    tracing::error!("Execution revert analysis:");
                    tracing::error!("  - Beacon address: {} (has code deployed)", beacon_address);
                    tracing::error!("  - PerpHook address: {}", state.perp_hook_address);
                    tracing::error!("  - Trading fee: {} bps", config.trading_fee_bps);
                    tracing::error!("  - Tick spacing: {}", config.tick_spacing);
                    tracing::error!("  - Starting price: {}", config.starting_sqrt_price_x96);
                    tracing::error!("  - Max margin: {} USDC", config.max_margin_usdc);
                }
                "Insufficient Funds" => {
                    tracing::error!("Troubleshooting hints:");
                    tracing::error!("  - Check wallet ETH balance for gas fees");
                    tracing::error!("  - Verify USDC balance if contract requires token transfers");
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
    let perp_id = parse_perp_created_event(&receipt, state.perp_hook_address)?;

    tracing::info!("Successfully deployed perp with ID: {}", perp_id);
    tracing::info!(
        "Perp is managed by PerpHook contract: {}",
        state.perp_hook_address
    );

    Ok(DeployPerpForBeaconResponse {
        perp_id: perp_id.to_string(),
        perp_hook_address: state.perp_hook_address.to_string(),
        transaction_hash: tx_hash.to_string(),
    })
}

// Contract error decoding utilities
struct ContractErrorDecoder;

impl ContractErrorDecoder {
    // Known PerpHook error signatures
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
                    "Unknown contract error (0xfb8f41b2) - pool: {pool_address}, value: {param2_value}. This error signature is not recognized in the PerpHook contract."
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
pub async fn deposit_liquidity_for_perp(
    state: &AppState,
    request: DepositLiquidityForPerpRequest,
) -> Result<DepositLiquidityForPerpResponse, String> {
    tracing::info!("Starting liquidity deposit for perp: {}", request.perp_id);

    // Parse the perp ID (PoolId as bytes32)
    let perp_id = match FixedBytes::<32>::from_str(&request.perp_id) {
        Ok(id) => id,
        Err(e) => {
            let error_msg = format!("Invalid perp ID '{}': {e}", request.perp_id);
            tracing::error!("{}", error_msg);
            return Err(error_msg);
        }
    };

    // Parse the margin amount (USDC in 6 decimals)
    let margin_amount_usdc = match request.margin_amount_usdc.parse::<u128>() {
        Ok(amount) => {
            if amount == 0 {
                let error_msg = "Margin amount cannot be zero".to_string();
                tracing::error!("{}", error_msg);
                return Err(error_msg);
            }
            amount
        }
        Err(e) => {
            let error_msg = format!(
                "Invalid margin amount '{}': {e}",
                request.margin_amount_usdc
            );
            tracing::error!("{}", error_msg);
            return Err(error_msg);
        }
    };

    // Validate margin amount against configuration
    if let Err(e) = state
        .perp_config
        .validate_leverage_bounds(margin_amount_usdc)
    {
        tracing::error!("Margin validation failed: {}", e);
        return Err(e);
    }

    // Convert margin to liquidity amount
    let liquidity_amount = margin_amount_usdc * state.perp_config.liquidity_scaling_factor;
    tracing::info!(
        "Margin {} USDC -> Liquidity {} (scaling factor: {})",
        margin_amount_usdc as f64 / 1_000_000.0,
        liquidity_amount,
        state.perp_config.liquidity_scaling_factor
    );

    // Validate liquidity bounds
    let (min_liquidity, max_liquidity) = state
        .perp_config
        .calculate_liquidity_bounds(margin_amount_usdc);
    if liquidity_amount < min_liquidity {
        let error_msg = format!(
            "Calculated liquidity {} is below minimum {} for {} USDC margin",
            liquidity_amount,
            min_liquidity,
            margin_amount_usdc as f64 / 1_000_000.0
        );
        tracing::error!("{}", error_msg);
        return Err(error_msg);
    }
    if liquidity_amount > max_liquidity {
        let error_msg = format!(
            "Calculated liquidity {} exceeds maximum {} for {} USDC margin",
            liquidity_amount,
            max_liquidity,
            margin_amount_usdc as f64 / 1_000_000.0
        );
        tracing::error!("{}", error_msg);
        return Err(error_msg);
    }

    // Check wallet balance
    match state.provider.get_balance(state.wallet_address).await {
        Ok(balance) => {
            let balance_f64 = balance.to::<u128>() as f64 / 1e18;
            tracing::info!("Wallet ETH balance: {:.6}", balance_f64);
        }
        Err(e) => tracing::warn!("Failed to get wallet balance: {}", e),
    }

    // Create contract instances
    let usdc_contract = IERC20::new(state.usdc_address, &*state.provider);
    let perp_hook_contract = IPerpHook::new(state.perp_hook_address, &*state.provider);

    // Check USDC balance
    let balance = usdc_contract
        .balanceOf(state.wallet_address)
        .call()
        .await
        .map_err(|e| format!("Failed to check USDC balance: {e}"))?;
    let margin_amount_usdc_u256 = U256::from(margin_amount_usdc);
    if balance < margin_amount_usdc_u256 {
        let balance_usdc = (balance.to::<u128>() as f64) / 1_000_000.0;
        let margin_usdc = (margin_amount_usdc_u256.to::<u128>() as f64) / 1_000_000.0;
        let error_msg =
            format!("Insufficient USDC balance: have {balance_usdc:.6}, need {margin_usdc:.6}");
        tracing::error!("{}", error_msg);
        return Err(error_msg);
    }

    // Check current USDC allowance for PerpHook and only approve if needed
    let allowance = usdc_contract
        .allowance(state.wallet_address, state.perp_hook_address)
        .call()
        .await
        .map_err(|e| format!("Failed to check USDC allowance: {e}"))?;
    let required_allowance = U256::from(margin_amount_usdc);

    let approval_tx_hash = if allowance < required_allowance {
        tracing::info!(
            "Insufficient allowance ({} < {}), approving USDC spending",
            allowance,
            required_allowance
        );

        // Execute USDC approval transaction (serialized)
        let approval_pending_tx = execute_transaction_serialized(async {
            usdc_contract
                .approve(state.perp_hook_address, required_allowance)
                .send()
                .await
                .map_err(|e| {
                    let error_type = match e.to_string().as_str() {
                        s if s.contains("insufficient funds") => "Insufficient Funds",
                        s if s.contains("gas") => "Gas Related Error",
                        s if s.contains("nonce") => "Nonce Error",
                        _ => "Approval Transaction Error",
                    };

                    let error_msg = format!("{error_type}: {e}");
                    tracing::error!("{}", error_msg);
                    tracing::error!("USDC approval failed: {:?}", e);

                    if let Some(revert_reason) = try_decode_revert_reason(&e) {
                        tracing::error!("Approval revert reason: {}", revert_reason);
                    }

                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    error_msg
                })
        })
        .await?;

        tracing::info!("USDC approval transaction sent, waiting for confirmation...");
        let approval_receipt =
            match timeout(Duration::from_secs(60), approval_pending_tx.get_receipt()).await {
                Ok(Ok(receipt)) => {
                    tracing::info!("USDC approval confirmed!");
                    receipt
                }
                Ok(Err(e)) => {
                    let error_msg = format!("USDC approval transaction failed: {e}");
                    tracing::error!("{}", error_msg);
                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    return Err(error_msg);
                }
                Err(_) => {
                    let error_msg = "Timeout waiting for USDC approval confirmation".to_string();
                    tracing::error!("{}", error_msg);
                    sentry::capture_message(&error_msg, sentry::Level::Error);
                    return Err(error_msg);
                }
            };

        let tx_hash = approval_receipt.transaction_hash;
        tracing::info!("USDC approval transaction confirmed: {:?}", tx_hash);
        tx_hash
    } else {
        tracing::info!(
            "Sufficient allowance ({} >= {}), skipping approval",
            allowance,
            required_allowance
        );
        Default::default() // No approval needed
    };

    // Execute liquidity deposit transaction (serialized)
    tracing::info!("Depositing liquidity for perp...");

    // Get tick bounds from config
    let tick_lower = Signed::<24, 1>::try_from(state.perp_config.default_tick_lower)
        .map_err(|e| format!("Invalid tick_lower: {e}"))?;
    let tick_upper = Signed::<24, 1>::try_from(state.perp_config.default_tick_upper)
        .map_err(|e| format!("Invalid tick_upper: {e}"))?;

    let deposit_pending_tx = execute_transaction_serialized(async {
        perp_hook_contract
            .openMakerPosition(
                perp_id,
                IPerpHook::OpenMakerPositionParams {
                    margin: margin_amount_usdc,
                    liquidity: liquidity_amount,
                    tickLower: tick_lower,
                    tickUpper: tick_upper,
                },
            )
            .send()
            .await
            .map_err(|e| {
                let error_type = match e.to_string().as_str() {
                    s if s.contains("execution reverted") => "Contract Execution Reverted",
                    s if s.contains("insufficient funds") => "Insufficient Funds",
                    s if s.contains("gas") => "Gas Related Error",
                    s if s.contains("nonce") => "Nonce Error",
                    _ => "Deposit Transaction Error",
                };

                let error_msg = format!("{error_type}: {e}");
                tracing::error!("{}", error_msg);
                tracing::error!("Liquidity deposit failed: {:?}", e);

                // Try to decode revert reason
                if let Some(revert_reason) = try_decode_revert_reason(&e) {
                    tracing::error!("Deposit revert reason: {}", revert_reason);
                }

                // Specific troubleshooting for deposit failures
                match error_type {
                    "Contract Execution Reverted" => {
                        tracing::error!("Troubleshooting hints:");
                        tracing::error!("  - Check if PerpHook contract is properly deployed");
                        tracing::error!(
                            "  - Verify perp ID {} exists and is valid",
                            request.perp_id
                        );
                        tracing::error!("  - Ensure USDC approval was successful");
                        tracing::error!(
                            "  - Check if liquidity amount {} is within bounds",
                            liquidity_amount
                        );
                        tracing::error!(
                            "  - Verify external contracts (PoolManager, Router) are accessible"
                        );
                        tracing::error!("  - Check PerpHook contract has required permissions");
                    }
                    "Insufficient Funds" => {
                        tracing::error!("  - Check wallet has sufficient ETH for gas fees");
                    }
                    _ => {}
                }

                sentry::capture_message(&error_msg, sentry::Level::Error);
                error_msg
            })
    })
    .await?;

    tracing::info!("Liquidity deposit transaction sent, waiting for confirmation...");
    let deposit_receipt =
        match timeout(Duration::from_secs(120), deposit_pending_tx.get_receipt()).await {
            Ok(Ok(receipt)) => {
                tracing::info!("Liquidity deposit confirmed!");
                receipt
            }
            Ok(Err(e)) => {
                let error_msg = format!("Liquidity deposit transaction failed: {e}");
                tracing::error!("{}", error_msg);
                sentry::capture_message(&error_msg, sentry::Level::Error);
                return Err(error_msg);
            }
            Err(_) => {
                let error_msg = "Timeout waiting for liquidity deposit confirmation".to_string();
                tracing::error!("{}", error_msg);
                sentry::capture_message(&error_msg, sentry::Level::Error);
                return Err(error_msg);
            }
        };

    let deposit_tx_hash = deposit_receipt.transaction_hash;
    tracing::info!(
        "Liquidity deposit transaction confirmed: {:?}",
        deposit_tx_hash
    );

    // Parse the MakerPositionOpened event
    let maker_position_id =
        parse_maker_position_opened_event(&deposit_receipt, state.perp_hook_address, perp_id)?;

    tracing::info!("Successfully deposited liquidity!");
    tracing::info!("Maker position ID: {}", maker_position_id);
    tracing::info!("Perp ID: {}", request.perp_id);
    tracing::info!(
        "Margin amount: {} USDC",
        margin_amount_usdc as f64 / 1_000_000.0
    );
    tracing::info!("Liquidity amount: {}", liquidity_amount);

    Ok(DepositLiquidityForPerpResponse {
        maker_position_id: maker_position_id.to_string(),
        approval_transaction_hash: approval_tx_hash.to_string(),
        deposit_transaction_hash: deposit_tx_hash.to_string(),
    })
}

// Helper function to execute batch liquidity deposits using multicall3 - single transaction with multiple calls
pub async fn batch_deposit_liquidity_with_multicall3(
    state: &AppState,
    _multicall_address: Address,
    deposits: &[DepositLiquidityForPerpRequest],
) -> Vec<(String, Result<String, String>)> {
    tracing::info!(
        "Starting batch liquidity deposit for {} perps using multicall3",
        deposits.len()
    );

    let mut results = Vec::with_capacity(deposits.len());

    // Process each deposit individually but collect for potential multicall optimization
    for deposit in deposits {
        let perp_id_str = deposit.perp_id.clone();
        tracing::info!("Processing liquidity deposit for perp: {}", perp_id_str);

        // Parse inputs with error handling
        let perp_id = match FixedBytes::<32>::from_str(&deposit.perp_id) {
            Ok(id) => id,
            Err(e) => {
                let error_msg = format!("Invalid perp ID '{}': {}", deposit.perp_id, e);
                tracing::error!("{}", error_msg);
                results.push((perp_id_str, Err(error_msg)));
                continue;
            }
        };

        let margin_amount_usdc = match deposit.margin_amount_usdc.parse::<u128>() {
            Ok(amount) => {
                if amount == 0 {
                    let error_msg = "Margin amount cannot be zero".to_string();
                    tracing::error!("{}", error_msg);
                    results.push((perp_id_str, Err(error_msg)));
                    continue;
                }
                amount
            }
            Err(e) => {
                let error_msg = format!(
                    "Invalid margin amount '{}': {}",
                    deposit.margin_amount_usdc, e
                );
                tracing::error!("{}", error_msg);
                results.push((perp_id_str, Err(error_msg)));
                continue;
            }
        };

        // Validate margin amount
        if let Err(e) = state
            .perp_config
            .validate_leverage_bounds(margin_amount_usdc)
        {
            tracing::error!("Margin validation failed for perp {}: {}", perp_id_str, e);
            results.push((perp_id_str, Err(e)));
            continue;
        }

        // Calculate liquidity
        let liquidity_amount = margin_amount_usdc * state.perp_config.liquidity_scaling_factor;
        tracing::info!(
            "Perp {}: Margin {} USDC -> Liquidity {}",
            perp_id_str,
            margin_amount_usdc as f64 / 1_000_000.0,
            liquidity_amount
        );

        // Validate liquidity bounds
        let (min_liquidity, max_liquidity) = state
            .perp_config
            .calculate_liquidity_bounds(margin_amount_usdc);
        if liquidity_amount < min_liquidity || liquidity_amount > max_liquidity {
            let error_msg = format!(
                "Liquidity {} out of bounds [{}, {}] for margin {}",
                liquidity_amount,
                min_liquidity,
                max_liquidity,
                margin_amount_usdc as f64 / 1_000_000.0
            );
            tracing::error!("{}", error_msg);
            results.push((perp_id_str, Err(error_msg)));
            continue;
        }

        // For now, execute each deposit individually (multicall optimization can be added later)
        // This maintains compatibility and allows for better error isolation
        let result =
            execute_individual_deposit(state, perp_id, margin_amount_usdc, liquidity_amount).await;

        results.push((perp_id_str, result));
    }

    tracing::info!("Batch liquidity deposit processing completed");
    results
}

// Helper function to execute individual liquidity deposit (used by batch processing)
async fn execute_individual_deposit(
    state: &AppState,
    perp_id: FixedBytes<32>,
    margin_amount_usdc: u128,
    liquidity_amount: u128,
) -> Result<String, String> {
    // Create contract instances
    let usdc_contract = IERC20::new(state.usdc_address, &*state.provider);
    let perp_hook_contract = IPerpHook::new(state.perp_hook_address, &*state.provider);

    // Check USDC balance
    let balance = usdc_contract
        .balanceOf(state.wallet_address)
        .call()
        .await
        .map_err(|e| format!("Failed to check USDC balance: {e}"))?;
    let margin_amount_usdc_u256 = U256::from(margin_amount_usdc);
    if balance < margin_amount_usdc_u256 {
        let balance_usdc = (balance.to::<u128>() as f64) / 1_000_000.0;
        let margin_usdc = (margin_amount_usdc_u256.to::<u128>() as f64) / 1_000_000.0;
        let error_msg =
            format!("Insufficient USDC balance: have {balance_usdc:.6}, need {margin_usdc:.6}");
        return Err(error_msg);
    }

    // Check current allowance and only approve if needed
    let allowance = usdc_contract
        .allowance(state.wallet_address, state.perp_hook_address)
        .call()
        .await
        .map_err(|e| format!("Failed to check USDC allowance: {e}"))?;
    let required_allowance = U256::from(margin_amount_usdc);

    if allowance < required_allowance {
        // Execute USDC approval only when needed
        let approval_result = execute_transaction_serialized(async {
            usdc_contract
                .approve(state.perp_hook_address, required_allowance)
                .send()
                .await
                .map_err(|e| {
                    let error_msg = format!("USDC approval failed: {e}");
                    if let Some(revert_reason) = try_decode_revert_reason(&e) {
                        tracing::error!("Approval revert: {}", revert_reason);
                    }
                    error_msg
                })
        })
        .await;

        let approval_pending_tx = match approval_result {
            Ok(tx) => tx,
            Err(e) => return Err(e),
        };

        // Wait for approval confirmation
        match timeout(Duration::from_secs(60), approval_pending_tx.get_receipt()).await {
            Ok(Ok(_receipt)) => {
                tracing::info!("USDC approval confirmed for {}", margin_amount_usdc);
            }
            Ok(Err(e)) => return Err(format!("USDC approval transaction failed: {e}")),
            Err(_) => return Err("Timeout waiting for USDC approval".to_string()),
        }
    } else {
        tracing::info!("Sufficient allowance, skipping approval");
    }

    // Execute liquidity deposit
    // Get tick bounds from config
    let tick_lower_batch = Signed::<24, 1>::try_from(state.perp_config.default_tick_lower).unwrap();
    let tick_upper_batch = Signed::<24, 1>::try_from(state.perp_config.default_tick_upper).unwrap();

    let deposit_result = execute_transaction_serialized(async {
        perp_hook_contract
            .openMakerPosition(
                perp_id,
                IPerpHook::OpenMakerPositionParams {
                    margin: margin_amount_usdc,
                    liquidity: liquidity_amount,
                    tickLower: tick_lower_batch,
                    tickUpper: tick_upper_batch,
                },
            )
            .send()
            .await
            .map_err(|e| {
                let error_msg = format!("Liquidity deposit failed: {e}");
                if let Some(revert_reason) = try_decode_revert_reason(&e) {
                    tracing::error!("Deposit revert: {}", revert_reason);
                }
                error_msg
            })
    })
    .await;

    let deposit_pending_tx = match deposit_result {
        Ok(tx) => tx,
        Err(e) => return Err(e),
    };

    // Wait for deposit confirmation
    let deposit_receipt =
        match timeout(Duration::from_secs(120), deposit_pending_tx.get_receipt()).await {
            Ok(Ok(receipt)) => receipt,
            Ok(Err(e)) => return Err(format!("Liquidity deposit transaction failed: {e}")),
            Err(_) => return Err("Timeout waiting for liquidity deposit".to_string()),
        };

    // Parse the MakerPositionOpened event
    let maker_position_id =
        parse_maker_position_opened_event(&deposit_receipt, state.perp_hook_address, perp_id)?;

    Ok(maker_position_id.to_string())
}

// Tests moved to tests/unit_tests/perp_operations_tests.rs
