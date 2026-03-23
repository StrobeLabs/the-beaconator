use alloy::primitives::{Address, FixedBytes, Signed, U256, Uint};
use alloy::providers::Provider;
use tracing;

use super::super::transaction::events::{
    parse_maker_position_opened_event, parse_perp_created_event,
};
use super::super::transaction::execution::is_nonce_error;
use super::validation::try_decode_revert_reason;
use crate::models::{AppState, DeployPerpForBeaconResponse, DepositLiquidityForPerpResponse};
use crate::routes::{IERC20, IPerpManager};

/// Deploys a perpetual contract for a specific beacon.
///
/// Creates a new perpetual pool using the PerpManager contract with modular plugin configuration.
/// Returns the perp ID and transaction hash on success.
pub async fn deploy_perp_for_beacon(
    state: &AppState,
    beacon_address: Address,
    fees_module: Address,
    margin_ratios_module: Address,
    lockup_period_module: Address,
    sqrt_price_impact_limit_module: Address,
) -> Result<DeployPerpForBeaconResponse, String> {
    tracing::info!("Starting perp deployment for beacon: {}", beacon_address);

    // Acquire a wallet from the pool
    let wallet_handle = state
        .wallets
        .manager
        .acquire_any_wallet()
        .await
        .map_err(|e| format!("Failed to acquire wallet: {e}"))?;

    let wallet_address = wallet_handle.address();
    tracing::info!("Acquired wallet {} for perp deployment", wallet_address);

    // Build provider with the acquired wallet
    let provider = wallet_handle
        .build_provider(&state.provider.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;

    // Log environment details
    tracing::info!("Environment details:");
    tracing::info!("  - PerpManager address: {}", state.contracts.perp_manager);
    tracing::info!("  - Wallet address: {}", wallet_address);
    tracing::info!("  - USDC address: {}", state.contracts.usdc);
    tracing::info!("Modular configuration:");
    tracing::info!("  - Fees module: {}", fees_module);
    tracing::info!("  - Margin ratios module: {}", margin_ratios_module);
    tracing::info!("  - Lockup period module: {}", lockup_period_module);
    tracing::info!(
        "  - Sqrt price impact limit module: {}",
        sqrt_price_impact_limit_module
    );
    // Check wallet balance first using read provider
    match state
        .provider
        .read_provider
        .get_balance(wallet_address)
        .await
    {
        Ok(balance) => {
            let balance_f64 = balance.to::<u128>() as f64 / 1e18;
            tracing::info!("Wallet balance: {:.6} ETH", balance_f64);
        }
        Err(e) => {
            tracing::warn!("Failed to get wallet balance: {}", e);
        }
    }

    // Create contract instance using the wallet's provider for transactions
    let contract = IPerpManager::new(state.contracts.perp_manager, &provider);

    // Validate beacon exists and has code deployed using read provider
    tracing::info!("Validating beacon address exists...");
    match state
        .provider
        .read_provider
        .get_code_at(beacon_address)
        .await
    {
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
        .read_provider
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

    // Try to call index() function to verify it's a beacon contract
    tracing::info!("Validating beacon contract has index() function...");
    let index_call_result = state
        .provider
        .read_provider
        .call(
            alloy::rpc::types::TransactionRequest::default()
                .to(beacon_address)
                .input(alloy::primitives::hex!("2986c0e5").to_vec().into()), // selector for index()
        )
        .await;

    match index_call_result {
        Ok(_) => {
            tracing::info!("Beacon contract has index() function");
        }
        Err(e) => {
            tracing::warn!("Beacon contract may not have index() function: {}", e);
            tracing::warn!(
                "  - The perp deployment may fail if PerpManager expects a standard beacon"
            );
        }
    }

    // Prepare the CreatePerpParams struct with modular configuration
    tracing::info!("CreatePerpParams parameters (5 fields - modular architecture):");
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

    let create_perp_params = IPerpManager::CreatePerpParams {
        beacon: beacon_address,
        fees: fees_module,
        marginRatios: margin_ratios_module,
        lockupPeriod: lockup_period_module,
        sqrtPriceImpactLimit: sqrt_price_impact_limit_module,
    };

    tracing::info!("CreatePerpParams struct prepared successfully");
    tracing::info!("Initiating createPerp transaction...");

    // Send the transaction and wait for confirmation
    tracing::info!("Sending createPerp transaction to PerpManager contract...");
    let pending_tx = contract
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
            tracing::error!("  - PerpManager address: {}", state.contracts.perp_manager);
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
                    tracing::error!("  - Verify beacon is not already registered with PerpManager");
                    tracing::error!("  - Check if beacon implements the expected interface");
                    tracing::error!("  - Verify PerpManager contract has required permissions");
                    tracing::error!("  - Verify module contracts are properly configured");

                    // Additional debugging for execution reverted
                    tracing::error!("Execution revert analysis:");
                    tracing::error!("  - Beacon address: {} (has code deployed)", beacon_address);
                    tracing::error!("  - PerpManager address: {}", state.contracts.perp_manager);
                    tracing::error!("  - Fees module: {}", fees_module);
                    tracing::error!("  - Margin ratios module: {}", margin_ratios_module);
                    tracing::error!("  - Lockup period module: {}", lockup_period_module);
                    tracing::error!(
                        "  - Sqrt price impact limit module: {}",
                        sqrt_price_impact_limit_module
                    );
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
        })?;

    tracing::info!("Transaction sent successfully, waiting for confirmation...");
    let pending_tx_hash = *pending_tx.tx_hash();
    tracing::info!("Transaction hash (pending): {:?}", pending_tx_hash);

    let receipt = crate::services::transaction::poll_for_receipt(
        &*state.provider.read_provider,
        pending_tx_hash,
        120,
    )
    .await
    .map_err(|e| {
        tracing::error!("{}", e);
        sentry::capture_message(&e, sentry::Level::Error);
        e
    })?;

    let tx_hash = receipt.transaction_hash;
    tracing::info!("Perp deployment transaction confirmed successfully!");
    tracing::info!("Final transaction hash: {:?}", tx_hash);
    tracing::info!(
        "Perp deployment confirmed in block {:?}",
        receipt.block_number
    );

    // Parse the perp ID from the PerpCreated event
    let perp_id = parse_perp_created_event(&receipt, state.contracts.perp_manager)?;

    tracing::info!("Successfully deployed perp with ID: {}", perp_id);
    tracing::info!(
        "Perp is managed by PerpManager contract: {}",
        state.contracts.perp_manager
    );

    Ok(DeployPerpForBeaconResponse {
        perp_id: perp_id.to_string(),
        perp_manager_address: state.contracts.perp_manager.to_string(),
        transaction_hash: tx_hash.to_string(),
    })
}

/// Helper function to deposit liquidity for a perp using configuration from AppState
pub async fn deposit_liquidity_for_perp(
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

    // Acquire a wallet from the pool
    let wallet_handle = state
        .wallets
        .manager
        .acquire_any_wallet()
        .await
        .map_err(|e| format!("Failed to acquire wallet: {e}"))?;

    let wallet_address = wallet_handle.address();
    tracing::info!("Acquired wallet {} for liquidity deposit", wallet_address);

    // Build provider with the acquired wallet
    let provider = wallet_handle
        .build_provider(&state.provider.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;

    // Create contract instance using the wallet's provider
    let contract = IPerpManager::new(state.contracts.perp_manager, &provider);

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
    let liquidity_raw = margin_amount_usdc * liquidity_scaling_factor;

    // uint120 max value: 2^120 - 1
    const MAX_UINT120: u128 = (1u128 << 120) - 1;
    if liquidity_raw > MAX_UINT120 {
        return Err(format!(
            "Computed liquidity {liquidity_raw} exceeds uint120 max ({MAX_UINT120}). Reduce margin amount."
        ));
    }

    // Set reasonable defaults for slippage protection (max values mean no limit)
    let max_amt0_in = u128::MAX;
    let max_amt1_in = u128::MAX;

    let open_maker_params = IPerpManager::OpenMakerPositionParams {
        holder: wallet_address,
        margin: margin_amount_usdc,
        liquidity: Uint::<120, 2>::from(liquidity_raw),
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
        liquidity_raw
    );

    // First, approve USDC spending by the PerpManager contract
    tracing::info!(
        "Approving USDC spending: {} USDC for PerpManager contract {}",
        margin_amount_usdc as f64 / 1_000_000.0,
        state.contracts.perp_manager
    );

    // USDC approval using acquired wallet
    let usdc_contract = IERC20::new(state.contracts.usdc, &provider);
    tracing::info!("Approving USDC spending with wallet {}", wallet_address);
    let pending_approval = match usdc_contract
        .approve(state.contracts.perp_manager, U256::from(margin_amount_usdc))
        .send()
        .await
    {
        Ok(pending) => Ok(pending),
        Err(e) => {
            let error_msg = format!("Failed to approve USDC spending: {e}");
            tracing::error!("{}", error_msg);
            tracing::error!("Make sure the wallet has sufficient USDC balance");

            // Check if nonce error
            if is_nonce_error(&error_msg) {
                tracing::warn!("Nonce error detected, transaction failed");
            }

            sentry::capture_message(&error_msg, sentry::Level::Error);
            Err(error_msg)
        }
    }?;

    tracing::info!("USDC approval transaction sent, waiting for confirmation...");
    let approval_tx_hash = *pending_approval.tx_hash();
    tracing::info!("USDC approval transaction hash: {:?}", approval_tx_hash);

    let approval_receipt = crate::services::transaction::poll_for_receipt(
        &*state.provider.read_provider,
        approval_tx_hash,
        150,
    )
    .await
    .map_err(|e| {
        tracing::error!("{}", e);
        sentry::capture_message(&e, sentry::Level::Error);
        e
    })?;

    // Send the openMakerPosition transaction
    tracing::info!("Opening maker position with wallet {}", wallet_address);
    let pending_tx = match contract
        .openMakerPos(perp_id, open_maker_params.clone())
        .send()
        .await
    {
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

            // Check if nonce error
            if is_nonce_error(&error_msg) {
                tracing::warn!("Nonce error detected, transaction failed");
            }

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

            sentry::capture_message(&error_msg, sentry::Level::Error);
            Err(error_msg)
        }
    }?;

    tracing::info!("Liquidity deposit transaction sent, waiting for confirmation...");
    let deposit_tx_hash = *pending_tx.tx_hash();
    tracing::info!("Liquidity deposit transaction hash: {:?}", deposit_tx_hash);

    let receipt = crate::services::transaction::poll_for_receipt(
        &*state.provider.read_provider,
        deposit_tx_hash,
        90,
    )
    .await
    .map_err(|e| {
        tracing::error!("{}", e);
        sentry::capture_message(&e, sentry::Level::Error);
        e
    })?;

    tracing::info!(
        "Liquidity deposit transaction confirmed with hash: {:?}",
        receipt.transaction_hash
    );

    // Parse the maker position ID from the MakerPositionOpened event
    let maker_pos_id =
        parse_maker_position_opened_event(&receipt, state.contracts.perp_manager, perp_id)?;

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
