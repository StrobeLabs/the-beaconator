use alloy::primitives::{Address, FixedBytes, Signed, U160, U256};
use alloy::providers::Provider;
use std::time::Duration;
use tokio::time::timeout;
use tracing;

use super::super::transaction::events::{
    parse_maker_position_opened_event, parse_perp_created_event,
};
use super::super::transaction::execution::{get_fresh_nonce_from_alternate, is_nonce_error};
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
                    tracing::error!("  - Verify beacon is not already registered with PerpManager");
                    tracing::error!("  - Check if beacon implements the expected interface");
                    tracing::error!("  - Verify PerpManager contract has required permissions");
                    tracing::error!("  - Verify module contracts are properly configured");

                    // Additional debugging for execution reverted
                    tracing::error!("Execution revert analysis:");
                    tracing::error!("  - Beacon address: {} (has code deployed)", beacon_address);
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

    // USDC approval with RPC fallback
    let usdc_contract = IERC20::new(state.usdc_address, &*state.provider);
    // Try primary RPC first
    tracing::info!("Approving USDC spending with primary RPC");
    let pending_approval = match usdc_contract
        .approve(state.perp_manager_address, U256::from(margin_amount_usdc))
        .send()
        .await
    {
        Ok(pending) => Ok(pending),
        Err(e) => {
            let error_msg = format!("Failed to approve USDC spending: {e}");
            tracing::error!("{}", error_msg);
            tracing::error!("Make sure the wallet has sufficient USDC balance");

            // Check if nonce error and sync if needed
            if is_nonce_error(&error_msg) {
                tracing::warn!("Nonce error detected, waiting before fallback");
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
    }?;

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

    // Send the openMakerPosition transaction with RPC fallback
    // Try primary RPC first
    tracing::info!("Opening maker position with primary RPC");
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

            // Check if nonce error and sync if needed
            if is_nonce_error(&error_msg) {
                tracing::warn!("Nonce error detected, waiting before fallback");
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
                                tracing::error!("  - Verify perp ID format (32-byte hex string)");
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
    }?;

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
