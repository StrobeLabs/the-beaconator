use alloy::primitives::{Address, FixedBytes, U256};
use alloy::providers::Provider;
use std::time::Duration;
use tokio::time::timeout;
use tracing;

use super::super::transaction::events::{parse_maker_opened_event, parse_perp_created_event};
use super::super::transaction::execution::is_nonce_error;
use super::validation::try_decode_revert_reason;
use crate::models::{AppState, DeployPerpForBeaconResponse, DepositLiquidityForPerpResponse};
use crate::routes::{IERC20, IPerp, IPerpFactory};

/// Deploys a per-market `Perp` contract via PerpFactory.createPerp (perpcity-contracts@v0.1.0).
///
/// Module addresses are taken from `state.contracts` (configured via env vars at startup).
/// On success, returns the new `Perp` contract address along with PoolId / sqrtPrice / tick
/// extracted from the `PerpCreated` event.
#[allow(clippy::too_many_arguments)]
pub async fn deploy_perp_for_beacon(
    state: &AppState,
    beacon_address: Address,
    owner: Address,
    name: String,
    symbol: String,
    token_uri: String,
    ema_window: u32,
    salt: FixedBytes<32>,
) -> Result<DeployPerpForBeaconResponse, String> {
    tracing::info!("Starting perp deployment for beacon: {}", beacon_address);

    let wallet_handle = state
        .wallets
        .manager
        .acquire_any_wallet()
        .await
        .map_err(|e| format!("Failed to acquire wallet: {e}"))?;

    let wallet_address = wallet_handle.address();
    tracing::info!("Acquired wallet {} for perp deployment", wallet_address);

    let provider = wallet_handle
        .build_provider(&state.provider.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;

    tracing::info!("Environment details:");
    tracing::info!("  - PerpFactory address: {}", state.contracts.perp_factory);
    tracing::info!("  - Wallet address: {}", wallet_address);
    tracing::info!("  - USDC address: {}", state.contracts.usdc);
    tracing::info!("Modules struct (server-configured):");
    tracing::info!("  - beacon: {}", beacon_address);
    tracing::info!("  - fees: {}", state.contracts.fees_module);
    tracing::info!("  - funding: {}", state.contracts.funding_module);
    tracing::info!("  - marginRatios: {}", state.contracts.margin_ratios_module);
    tracing::info!("  - priceImpact: {}", state.contracts.price_impact_module);
    tracing::info!("  - pricing: {}", state.contracts.pricing_module);

    if let Ok(balance) = state
        .provider
        .read_provider
        .get_balance(wallet_address)
        .await
    {
        let balance_f64 = balance.to::<u128>() as f64 / 1e18;
        tracing::info!("Wallet balance: {:.6} ETH", balance_f64);
    }

    // Verify the beacon contract has code deployed.
    match state
        .provider
        .read_provider
        .get_code_at(beacon_address)
        .await
    {
        Ok(code) if code.is_empty() => {
            let error_msg =
                format!("Beacon address {beacon_address} has no deployed code (not a contract)");
            tracing::error!("{}", error_msg);
            return Err(error_msg);
        }
        Ok(code) => {
            tracing::info!(
                "Beacon address {} has deployed code ({} bytes)",
                beacon_address,
                code.len()
            );
        }
        Err(e) => {
            let error_msg = format!("Failed to check beacon address {beacon_address}: {e}");
            tracing::error!("{}", error_msg);
            return Err(error_msg);
        }
    }

    let factory = IPerpFactory::new(state.contracts.perp_factory, &provider);

    let modules = IPerpFactory::Modules {
        beacon: beacon_address,
        fees: state.contracts.fees_module,
        funding: state.contracts.funding_module,
        marginRatios: state.contracts.margin_ratios_module,
        priceImpact: state.contracts.price_impact_module,
        pricing: state.contracts.pricing_module,
    };

    // emaWindow is encoded as uint24 on-chain; verify before sending so the revert is local.
    if ema_window == 0 {
        return Err("ema_window must be > 0 (uint24)".to_string());
    }
    if ema_window > 0xFF_FFFF {
        return Err(format!(
            "ema_window {ema_window} exceeds uint24 max (16777215)"
        ));
    }
    let ema_window_u24 = alloy::primitives::Uint::<24, 1>::from(ema_window);

    tracing::info!("Sending createPerp transaction to PerpFactory...");
    let pending_tx = factory
        .createPerp(
            owner,
            name.clone(),
            symbol.clone(),
            token_uri.clone(),
            modules,
            ema_window_u24,
            salt,
        )
        .send()
        .await
        .map_err(|e| {
            let mut error_msg = format!("createPerp send failed: {e}");
            if let Some(decoded) = try_decode_revert_reason(&e) {
                error_msg = format!("createPerp reverted: {decoded}");
            }
            tracing::error!("{}", error_msg);
            tracing::error!("Context:");
            tracing::error!("  - PerpFactory: {}", state.contracts.perp_factory);
            tracing::error!("  - Beacon: {}", beacon_address);
            tracing::error!("  - Owner: {}", owner);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            error_msg
        })?;

    let pending_tx_hash = *pending_tx.tx_hash();
    tracing::info!("createPerp tx hash: {:?}", pending_tx_hash);

    let receipt = match timeout(Duration::from_secs(120), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => receipt,
        Ok(Err(e)) => {
            tracing::warn!("get_receipt() failed for createPerp: {}", e);
            match timeout(
                Duration::from_secs(30),
                state
                    .provider
                    .read_provider
                    .get_transaction_receipt(pending_tx_hash),
            )
            .await
            {
                Ok(Ok(Some(r))) => r,
                Ok(Ok(None)) => {
                    let msg =
                        format!("createPerp transaction {pending_tx_hash} not found on-chain");
                    sentry::capture_message(&msg, sentry::Level::Error);
                    return Err(msg);
                }
                Ok(Err(e)) => {
                    let msg =
                        format!("Failed to check createPerp tx {pending_tx_hash} on-chain: {e}");
                    sentry::capture_message(&msg, sentry::Level::Error);
                    return Err(msg);
                }
                Err(_) => {
                    let msg = format!("Timeout checking createPerp tx {pending_tx_hash} on-chain");
                    sentry::capture_message(&msg, sentry::Level::Error);
                    return Err(msg);
                }
            }
        }
        Err(_) => {
            let msg = "Timeout waiting for createPerp receipt".to_string();
            sentry::capture_message(&msg, sentry::Level::Error);
            return Err(msg);
        }
    };

    let tx_hash = receipt.transaction_hash;
    tracing::info!("createPerp confirmed in block {:?}", receipt.block_number);

    let event = parse_perp_created_event(&receipt, state.contracts.perp_factory)?;

    tracing::info!("Deployed Perp at {}", event.perp);
    tracing::info!("PoolId: {}", event.pool_id);

    Ok(DeployPerpForBeaconResponse {
        perp_address: event.perp.to_string(),
        pool_id: format!("{:#x}", event.pool_id),
        perp_factory_address: state.contracts.perp_factory.to_string(),
        initial_index: event.initial_index.to_string(),
        ema_window,
        sqrt_price_x96: event.sqrt_price_x96.to_string(),
        tick: event.tick,
        transaction_hash: tx_hash.to_string(),
    })
}

/// Opens a maker liquidity position on a per-market `Perp` contract.
///
/// Approves USDC against the per-perp contract address (which calls `safeTransferFrom` from
/// `msg.sender`), then sends `Perp.openMaker(OpenMakerParams)`.
#[allow(clippy::too_many_arguments)]
pub async fn deposit_liquidity_for_perp(
    state: &AppState,
    perp_address: Address,
    margin_amount_usdc: u128,
    tick_spacing: i32,
    tick_lower: i32,
    tick_upper: i32,
) -> Result<DepositLiquidityForPerpResponse, String> {
    tracing::info!(
        "Opening maker on Perp {} with margin {}",
        perp_address,
        margin_amount_usdc
    );

    let wallet_handle = state
        .wallets
        .manager
        .acquire_any_wallet()
        .await
        .map_err(|e| format!("Failed to acquire wallet: {e}"))?;

    let wallet_address = wallet_handle.address();
    tracing::info!("Acquired wallet {} for liquidity deposit", wallet_address);

    let provider = wallet_handle
        .build_provider(&state.provider.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;

    let perp = IPerp::new(perp_address, &provider);

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

    // Conservative liquidity scaling: USDC margin (6 decimals) -> AMM liquidity unit.
    let liquidity_scaling_factor = 500_000u128;
    let liquidity_raw = margin_amount_usdc
        .checked_mul(liquidity_scaling_factor)
        .ok_or_else(|| "liquidity scaling overflow".to_string())?;

    // v0.1.0 widened OpenMakerParams.liquidity from uint120 to uint128 — `liquidity_raw` is
    // already u128, so the contract bound is trivially satisfied. Documented for posterity:
    // the upstream cap is u128::MAX. The earlier u120 cap that lived here is no longer required.

    // Slippage protection defaults: u256::MAX = "no limit". Caller-supplied limits could be
    // wired in once the request DTO carries them through.
    let max_amt0_in = U256::MAX;
    let max_amt1_in = U256::MAX;

    let open_maker_params = IPerp::OpenMakerParams {
        holder: wallet_address,
        margin: margin_amount_usdc,
        tickLower: alloy::primitives::Signed::<24, 1>::try_from(tick_lower)
            .map_err(|e| format!("Invalid tick lower: {e}"))?,
        tickUpper: alloy::primitives::Signed::<24, 1>::try_from(tick_upper)
            .map_err(|e| format!("Invalid tick upper: {e}"))?,
        liquidity: liquidity_raw,
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

    // The per-Perp contract calls safeTransferFrom(USDC, msg.sender, address(this), ...).
    // So the approve target is the per-Perp contract address, NOT the factory.
    tracing::info!(
        "Approving USDC ({} USDC) for Perp contract {}",
        margin_amount_usdc as f64 / 1_000_000.0,
        perp_address
    );

    let usdc_contract = IERC20::new(state.contracts.usdc, &provider);
    let pending_approval = usdc_contract
        .approve(perp_address, U256::from(margin_amount_usdc))
        .send()
        .await
        .map_err(|e| {
            let error_msg = format!("Failed to approve USDC spending: {e}");
            tracing::error!("{}", error_msg);
            if is_nonce_error(&error_msg) {
                tracing::warn!("Nonce error detected, transaction failed");
            }
            sentry::capture_message(&error_msg, sentry::Level::Error);
            error_msg
        })?;

    let approval_tx_hash = *pending_approval.tx_hash();
    tracing::info!("USDC approval tx hash: {:?}", approval_tx_hash);

    let approval_receipt =
        match timeout(Duration::from_secs(150), pending_approval.get_receipt()).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                tracing::warn!("get_receipt() failed for USDC approval: {}", e);
                wait_for_receipt(state, approval_tx_hash, "USDC approval").await?
            }
            Err(_) => {
                tracing::warn!("Initial get_receipt() timed out for USDC approval, polling...");
                wait_for_receipt(state, approval_tx_hash, "USDC approval").await?
            }
        };

    tracing::info!("Opening maker position with wallet {}", wallet_address);
    let pending_tx = perp
        .openMaker(open_maker_params.clone())
        .send()
        .await
        .map_err(|e| {
            let mut error_msg = format!("openMaker send failed: {e}");
            if let Some(decoded) = try_decode_revert_reason(&e) {
                error_msg = format!("openMaker reverted: {decoded}");
            }
            tracing::error!("{}", error_msg);
            if is_nonce_error(&error_msg) {
                tracing::warn!("Nonce error detected, transaction failed");
            }
            sentry::capture_message(&error_msg, sentry::Level::Error);
            error_msg
        })?;

    let deposit_tx_hash = *pending_tx.tx_hash();
    tracing::info!("openMaker tx hash: {:?}", deposit_tx_hash);

    let receipt = match timeout(Duration::from_secs(90), pending_tx.get_receipt()).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            tracing::warn!("get_receipt() failed for openMaker: {}", e);
            wait_for_receipt(state, deposit_tx_hash, "openMaker").await?
        }
        Err(_) => {
            let msg = "Timeout waiting for openMaker receipt".to_string();
            tracing::error!("{}", msg);
            return Err(msg);
        }
    };

    tracing::info!("openMaker confirmed: {:?}", receipt.transaction_hash);

    let pos_id = parse_maker_opened_event(&receipt, perp_address)?;
    tracing::info!("Maker position opened with posId {}", pos_id);

    Ok(DepositLiquidityForPerpResponse {
        maker_position_id: pos_id.to_string(),
        approval_transaction_hash: approval_receipt.transaction_hash.to_string(),
        deposit_transaction_hash: receipt.transaction_hash.to_string(),
    })
}

/// Poll the read provider for a transaction receipt with progressive backoff.
async fn wait_for_receipt(
    state: &AppState,
    tx_hash: alloy::primitives::FixedBytes<32>,
    label: &str,
) -> Result<alloy::rpc::types::TransactionReceipt, String> {
    let timeout_seconds = [15u64, 30u64, 60u64];
    for (attempt, secs) in timeout_seconds.iter().enumerate() {
        tracing::info!(
            "{} receipt attempt {}/{} ({}s timeout)",
            label,
            attempt + 1,
            timeout_seconds.len(),
            secs
        );
        match timeout(
            Duration::from_secs(*secs),
            state
                .provider
                .read_provider
                .get_transaction_receipt(tx_hash),
        )
        .await
        {
            Ok(Ok(Some(receipt))) => return Ok(receipt),
            Ok(Ok(None)) => {
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Ok(Err(e)) => {
                let msg = format!("Failed to query {label} receipt {tx_hash}: {e}");
                tracing::error!("{}", msg);
                return Err(msg);
            }
            Err(_) => {
                tracing::warn!("Timeout on attempt {}, retrying...", attempt + 1);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
    let msg = format!("{label} receipt {tx_hash} not found after retries");
    tracing::error!("{}", msg);
    Err(msg)
}
