use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use rocket::serde::json::Json;
use rocket::{State, http::Status, post};
use rocket_okapi::openapi;
use std::str::FromStr;
use std::time::Duration;
use tokio::time::timeout;
use tracing;

/// How long to wait for each funding transfer (ETH, USDC) to confirm.
const FUNDING_RECEIPT_TIMEOUT: Duration = Duration::from_secs(120);

use super::{IERC20, ITestnetUSDC};
use crate::guards::{AdminToken, ApiToken};
use crate::models::{
    ApiResponse, AppState, FundBonusWalletRequest, FundGuestWalletRequest, TopUpPoolRequest,
};

/// Default per-wallet USDC balance target for `/top_up_pool`: 10,000 USDC.
const DEFAULT_TOP_UP_USDC_TARGET: u128 = 10_000_000_000;

/// Production chain ids the beaconator can target. Any chain id NOT in the testnet/local
/// allow-list (Arbitrum Sepolia = 421614, Anvil default = 31337) is treated as production
/// so a future mainnet addition fails closed instead of accidentally unlocking the funding
/// endpoint.
fn is_production_chain(chain_id: u64) -> bool {
    !matches!(chain_id, 421614 | 31337)
}

/// Funds a guest wallet with USDC and ETH.
///
/// Transfers the specified amounts of USDC and ETH from the beaconator wallet
/// to the guest wallet address. Validates transfer limits and available balances.
#[openapi(tag = "Wallet")]
#[post("/fund_guest_wallet", format = "json", data = "<request>")]
pub async fn fund_guest_wallet(
    state: &State<AppState>,
    request: Json<FundGuestWalletRequest>,
    _token: ApiToken,
) -> Result<Json<ApiResponse<String>>, (Status, Json<ApiResponse<String>>)> {
    tracing::info!("Received request: POST /fund_guest_wallet");

    // Hard-disable guest-wallet funding on production chains. The endpoint pulls real ETH +
    // USDC from a hot wallet — fine on Arbitrum Sepolia (chain 421614) or local Anvil, but a
    // foot-gun on Arbitrum One (chain 42161). The chain id is set from ENV at startup and
    // cannot be overridden per request, so this is the canonical mainnet check.
    if is_production_chain(state.provider.chain_id) {
        let error_msg = format!(
            "fund_guest_wallet is disabled on chain id {} (production network); \
             this endpoint only runs on Arbitrum Sepolia / local Anvil",
            state.provider.chain_id
        );
        tracing::error!("{}", error_msg);
        return Err((
            Status::Forbidden,
            Json(ApiResponse {
                success: false,
                data: None,
                message: error_msg,
            }),
        ));
    }
    let wallet_address = match Address::from_str(&request.wallet_address) {
        Ok(addr) => addr,
        Err(e) => {
            return Err((
                Status::BadRequest,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!("Invalid wallet address: {e}"),
                }),
            ));
        }
    };

    // Parse amounts
    let usdc_amount = match request.usdc_amount.parse::<u128>() {
        Ok(amount) => amount,
        Err(e) => {
            return Err((
                Status::BadRequest,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!("Invalid USDC amount: {e}"),
                }),
            ));
        }
    };

    let eth_amount = match request.eth_amount.parse::<u128>() {
        Ok(amount) => amount,
        Err(e) => {
            return Err((
                Status::BadRequest,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!("Invalid ETH amount: {e}"),
                }),
            ));
        }
    };

    // Check transfer limits
    if usdc_amount > state.wallets.usdc_transfer_limit {
        return Err((
            Status::BadRequest,
            Json(ApiResponse {
                success: false,
                data: None,
                message: format!(
                    "USDC amount exceeds limit. Requested: {} USDC, Limit: {} USDC",
                    usdc_amount / 1_000_000,
                    state.wallets.usdc_transfer_limit / 1_000_000
                ),
            }),
        ));
    }

    if eth_amount > state.wallets.eth_transfer_limit {
        return Err((
            Status::BadRequest,
            Json(ApiResponse {
                success: false,
                data: None,
                message: format!(
                    "ETH amount exceeds limit. Requested: {} ETH, Limit: {} ETH",
                    alloy::primitives::utils::format_ether(U256::from(eth_amount)),
                    alloy::primitives::utils::format_ether(U256::from(
                        state.wallets.eth_transfer_limit
                    ))
                ),
            }),
        ));
    }

    tracing::info!(
        "Funding guest wallet: {} with {} USDC and {} ETH",
        wallet_address,
        usdc_amount / 1_000_000,
        alloy::primitives::utils::format_ether(U256::from(eth_amount))
    );

    // Acquire a pool wallet and verify it has both funds — before any transfer — so
    // the ETH/USDC balances we check are the ones that will actually fund the
    // transfer. The measurement signer (PRIVATE_KEY) never sends funds; all sends go
    // through the KMS-capable pool. WalletHandle already carries the distributed lock
    // plus a background heartbeat that extends it, so no separate lock/heartbeat
    // management is needed here — the wallet stays reserved until `wallet_handle`
    // drops.
    //
    // Selection is a bounded loop over the pool (one attempt per wallet, at most):
    // `acquire_wallet_for_usdc` orders candidates by cached USDC balance descending
    // (spreading drain across the pool instead of always hitting the same wallet —
    // see the 2026-06-30 testnet freeze), then this fresh on-chain check verifies
    // that cache (which can be up to one sweep interval stale). A wallet that fails
    // either check is excluded and the next candidate is tried; only once every
    // wallet in the pool has been tried does this return the insufficient-balance
    // error below.
    let max_wallet_attempts = state.wallets.manager.signer_addresses().len().max(1);
    let mut excluded_wallets: std::collections::HashSet<Address> = std::collections::HashSet::new();
    let mut wallet_handle = None;

    for attempt in 1..=max_wallet_attempts {
        let handle = state
            .wallets
            .manager
            .acquire_wallet_for_usdc(U256::from(usdc_amount), &excluded_wallets)
            .await
            .map_err(|e| {
                let detailed_error = format!("Failed to acquire pool wallet: {e}");
                tracing::error!("{}", detailed_error);
                (
                    Status::ServiceUnavailable,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        message: "Funding wallet temporarily unavailable".to_string(),
                    }),
                )
            })?;
        let candidate = handle.address();
        let last_attempt = attempt == max_wallet_attempts;

        // Check pool wallet ETH balance using read provider
        let eth_balance = match state.provider.read_provider.get_balance(candidate).await {
            Ok(balance) => balance,
            Err(e) => {
                let detailed_error = format!("Failed to get ETH balance: {e}");
                tracing::error!("{}", detailed_error);
                return Err((
                    Status::InternalServerError,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        message: "Failed to retrieve ETH balance".to_string(),
                    }),
                ));
            }
        };

        // Check if we have enough ETH: the transfer amount PLUS the reserve
        // floor the wallet must retain for beacon-update gas. Without the
        // reserve, faucet traffic can drain the pool below the
        // BeaconatorWalletGasLow paging threshold and freeze beacon updates.
        let eth_required =
            U256::from(eth_amount) + U256::from(state.wallets.faucet_reserve_eth_wei);
        if eth_balance < eth_required {
            tracing::warn!(
                "Pool wallet {} cannot fund guest without breaching the ETH reserve. \
                 Have: {} ETH, Need: {} ETH (transfer + {} ETH reserve)",
                candidate,
                alloy::primitives::utils::format_ether(eth_balance),
                alloy::primitives::utils::format_ether(eth_required),
                alloy::primitives::utils::format_ether(U256::from(
                    state.wallets.faucet_reserve_eth_wei
                ))
            );
            if !last_attempt {
                excluded_wallets.insert(candidate);
                drop(handle);
                continue;
            }
            return Err((
                Status::ServiceUnavailable,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!(
                        "Guest funding refused: every pool wallet is at its ETH reserve floor \
                         ({} ETH, kept for beacon gas). Top up the pool and retry.",
                        alloy::primitives::utils::format_ether(U256::from(
                            state.wallets.faucet_reserve_eth_wei
                        ))
                    ),
                }),
            ));
        }

        // Check USDC balance using read provider
        let usdc_read_contract = IERC20::new(state.contracts.usdc, &*state.provider.read_provider);
        let usdc_balance = match usdc_read_contract.balanceOf(candidate).call().await {
            Ok(result) => result,
            Err(e) => {
                let detailed_error = format!("Failed to get USDC balance: {e}");
                tracing::error!("{}", detailed_error);
                return Err((
                    Status::InternalServerError,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        message: "Failed to retrieve USDC balance".to_string(),
                    }),
                ));
            }
        };

        // Check if we have enough USDC
        if usdc_balance < U256::from(usdc_amount) {
            tracing::warn!(
                "Insufficient USDC balance in pool wallet {}. Have: {} USDC, Need: {} USDC",
                candidate,
                usdc_balance / U256::from(1_000_000),
                usdc_amount / 1_000_000
            );
            if !last_attempt {
                excluded_wallets.insert(candidate);
                drop(handle);
                continue;
            }
            return Err((
                Status::InternalServerError,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!(
                        "Insufficient USDC balance. Have: {} USDC, Need: {} USDC",
                        usdc_balance / U256::from(1_000_000), // Convert to human readable
                        usdc_amount / 1_000_000
                    ),
                }),
            ));
        }

        wallet_handle = Some(handle);
        break;
    }

    let wallet_handle =
        wallet_handle.expect("balance-check retry loop must return or break with a wallet handle");

    // Build a provider from the pool wallet's signer (local key or KMS, depending on
    // deployment) to send the two on-chain transfers below.
    let funding_provider = wallet_handle
        .build_provider(&state.provider.rpc_url)
        .map_err(|e| {
            let detailed_error = format!("Failed to build funding provider: {e}");
            tracing::error!("{}", detailed_error);
            (
                Status::InternalServerError,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: "Server RPC configuration is invalid".to_string(),
                }),
            )
        })?;

    // Send ETH using funding provider
    let tx_request = TransactionRequest::default()
        .to(wallet_address)
        .value(U256::from(eth_amount));

    let eth_tx_hash = match funding_provider.send_transaction(tx_request).await {
        Ok(pending) => {
            let tx_hash = *pending.tx_hash();
            match timeout(FUNDING_RECEIPT_TIMEOUT, pending.get_receipt()).await {
                Ok(Ok(receipt)) => receipt.transaction_hash,
                Ok(Err(e)) => {
                    let detailed_error = format!("Failed to get ETH transaction receipt: {e}");
                    tracing::error!("{}", detailed_error);
                    return Err((
                        Status::InternalServerError,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            message: format!(
                                "ETH transfer sent (tx {tx_hash:?}) but confirmation failed; \
                                 USDC was NOT sent — verify on-chain before retrying to avoid \
                                 double-funding"
                            ),
                        }),
                    ));
                }
                Err(_) => {
                    let detailed_error = format!(
                        "Timeout waiting for ETH transfer receipt (tx {tx_hash:?}) after {}s",
                        FUNDING_RECEIPT_TIMEOUT.as_secs()
                    );
                    tracing::error!("{}", detailed_error);
                    return Err((
                        Status::InternalServerError,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            message: format!(
                                "ETH transfer unconfirmed after {}s (tx {tx_hash:?}); USDC was \
                                 NOT sent — verify on-chain before retrying to avoid double-funding",
                                FUNDING_RECEIPT_TIMEOUT.as_secs()
                            ),
                        }),
                    ));
                }
            }
        }
        Err(e) => {
            let detailed_error = format!("Failed to send ETH: {e}");
            tracing::error!("{}", detailed_error);
            return Err((
                Status::InternalServerError,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: "Failed to send ETH".to_string(),
                }),
            ));
        }
    };

    tracing::info!("ETH transfer hash: {:?}", eth_tx_hash);

    // The ETH transfer may have taken longer than the lock TTL; abort before the
    // second transaction if the heartbeat observed the lock as lost.
    if let Err(e) = wallet_handle.ensure_lock_held() {
        let detailed_error = format!("Pool wallet lock lost before USDC transfer: {e}");
        tracing::error!("{}", detailed_error);
        return Err((
            Status::InternalServerError,
            Json(ApiResponse {
                success: false,
                data: None,
                message: format!(
                    "ETH sent (tx {eth_tx_hash:?}), but USDC transfer was aborted: {e}"
                ),
            }),
        ));
    }

    // Send USDC using funding provider
    let usdc_send_contract = IERC20::new(state.contracts.usdc, &funding_provider);
    let usdc_receipt = match usdc_send_contract
        .transfer(wallet_address, U256::from(usdc_amount))
        .send()
        .await
    {
        Ok(pending) => {
            let usdc_tx_hash = *pending.tx_hash();
            match timeout(FUNDING_RECEIPT_TIMEOUT, pending.get_receipt()).await {
                Ok(Ok(receipt)) => receipt,
                Ok(Err(e)) => {
                    let detailed_error = format!("Failed to get USDC transaction receipt: {e}");
                    tracing::error!("{}", detailed_error);
                    return Err((
                        Status::InternalServerError,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            message: format!(
                                "ETH sent (tx {eth_tx_hash:?}), USDC transfer confirmation \
                                 failed (tx {usdc_tx_hash:?}) — verify on-chain before retrying \
                                 to avoid double-funding"
                            ),
                        }),
                    ));
                }
                Err(_) => {
                    let detailed_error = format!(
                        "Timeout waiting for USDC transfer receipt (tx {usdc_tx_hash:?}) after {}s",
                        FUNDING_RECEIPT_TIMEOUT.as_secs()
                    );
                    tracing::error!("{}", detailed_error);
                    return Err((
                        Status::InternalServerError,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            message: format!(
                                "ETH sent (tx {eth_tx_hash:?}), USDC transfer unconfirmed after \
                                 {}s (tx {usdc_tx_hash:?}) — verify on-chain before retrying to \
                                 avoid double-funding",
                                FUNDING_RECEIPT_TIMEOUT.as_secs()
                            ),
                        }),
                    ));
                }
            }
        }
        Err(e) => {
            let detailed_error = format!("Failed to send USDC: {e}");
            tracing::error!("{}", detailed_error);
            return Err((
                Status::InternalServerError,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!("ETH sent (tx {eth_tx_hash:?}), but USDC send failed"),
                }),
            ));
        }
    };

    tracing::info!("USDC transfer hash: {:?}", usdc_receipt.transaction_hash);

    Ok(Json(ApiResponse {
        success: true,
        data: Some(format!(
            "Successfully funded wallet {} with {} USDC and {} ETH. ETH tx: {:?}, USDC tx: {:?}",
            wallet_address,
            usdc_amount / 1_000_000,
            alloy::primitives::utils::format_ether(U256::from(eth_amount)),
            eth_tx_hash,
            usdc_receipt.transaction_hash
        )),
        message: "Guest wallet funded successfully".to_string(),
    }))
}

/// Funds a wallet with the new-user bonus USDC (mainnet-capable).
///
/// The sibling of `fund_guest_wallet` for the real-money $50 bonus. Differences:
///   - USDC only, NO ETH leg — the recipient is a smart account whose trades are
///     paymaster-sponsored, so it never needs gas.
///   - Runs on ALL chains, including Arbitrum One: there is NO production
///     guardrail here, because disbursing real mainnet USDC is the entire point.
///     The real-money safety lives in (a) the tighter `usdc_bonus_limit` cap,
///     (b) the bearer-token auth, and (c) the caller's own kill-switch + atomic
///     single-use claim. The faucet route keeps its mainnet guard untouched.
#[openapi(tag = "Wallet")]
#[post("/fund_bonus_wallet", format = "json", data = "<request>")]
pub async fn fund_bonus_wallet(
    state: &State<AppState>,
    request: Json<FundBonusWalletRequest>,
    _token: ApiToken,
) -> Result<Json<ApiResponse<String>>, (Status, Json<ApiResponse<String>>)> {
    tracing::info!("Received request: POST /fund_bonus_wallet");

    let wallet_address = match Address::from_str(&request.wallet_address) {
        Ok(addr) => addr,
        Err(e) => {
            return Err((
                Status::BadRequest,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!("Invalid wallet address: {e}"),
                }),
            ));
        }
    };

    let usdc_amount = match request.usdc_amount.parse::<u128>() {
        Ok(amount) => amount,
        Err(e) => {
            return Err((
                Status::BadRequest,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!("Invalid USDC amount: {e}"),
                }),
            ));
        }
    };

    // Bound each payout by the dedicated bonus cap (real money — fail closed).
    if usdc_amount == 0 || usdc_amount > state.wallets.usdc_bonus_limit {
        return Err((
            Status::BadRequest,
            Json(ApiResponse {
                success: false,
                data: None,
                message: format!(
                    "USDC amount out of range. Requested: {} USDC, Limit: {} USDC",
                    usdc_amount / 1_000_000,
                    state.wallets.usdc_bonus_limit / 1_000_000
                ),
            }),
        ));
    }

    tracing::info!(
        "Funding bonus wallet: {} with {} USDC",
        wallet_address,
        usdc_amount / 1_000_000
    );

    // Acquire a pool wallet and verify its USDC balance — before the transfer — so
    // the balance we check is the one that will actually fund it. The measurement
    // signer (PRIVATE_KEY) never sends funds; all sends go through the KMS-capable
    // pool. WalletHandle already carries the distributed lock plus a background
    // heartbeat that extends it, so no separate lock/heartbeat management is needed
    // here.
    //
    // Same bounded-loop selection as `fund_guest_wallet`: `acquire_wallet_for_usdc`
    // orders candidates by cached USDC descending to spread drain across the pool,
    // this fresh on-chain check verifies the (possibly stale) cache, and a wallet
    // that fails it is excluded in favor of the next candidate.
    let max_wallet_attempts = state.wallets.manager.signer_addresses().len().max(1);
    let mut excluded_wallets: std::collections::HashSet<Address> = std::collections::HashSet::new();
    let mut wallet_handle = None;

    for attempt in 1..=max_wallet_attempts {
        let handle = state
            .wallets
            .manager
            .acquire_wallet_for_usdc(U256::from(usdc_amount), &excluded_wallets)
            .await
            .map_err(|e| {
                let detailed_error = format!("Failed to acquire pool wallet: {e}");
                tracing::error!("{}", detailed_error);
                (
                    Status::ServiceUnavailable,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        message: "Funding wallet temporarily unavailable".to_string(),
                    }),
                )
            })?;
        let candidate = handle.address();
        let last_attempt = attempt == max_wallet_attempts;

        // Check pool wallet USDC balance using read provider
        let usdc_read_contract = IERC20::new(state.contracts.usdc, &*state.provider.read_provider);
        let usdc_balance = match usdc_read_contract.balanceOf(candidate).call().await {
            Ok(result) => result,
            Err(e) => {
                let detailed_error = format!("Failed to get USDC balance: {e}");
                tracing::error!("{}", detailed_error);
                return Err((
                    Status::InternalServerError,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        message: "Failed to retrieve USDC balance".to_string(),
                    }),
                ));
            }
        };

        if usdc_balance < U256::from(usdc_amount) {
            tracing::warn!(
                "Insufficient USDC balance in pool wallet {}. Have: {} USDC, Need: {} USDC",
                candidate,
                usdc_balance / U256::from(1_000_000),
                usdc_amount / 1_000_000
            );
            if !last_attempt {
                excluded_wallets.insert(candidate);
                drop(handle);
                continue;
            }
            return Err((
                Status::InternalServerError,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!(
                        "Insufficient USDC balance. Have: {} USDC, Need: {} USDC",
                        usdc_balance / U256::from(1_000_000),
                        usdc_amount / 1_000_000
                    ),
                }),
            ));
        }

        wallet_handle = Some(handle);
        break;
    }

    let wallet_handle =
        wallet_handle.expect("balance-check retry loop must return or break with a wallet handle");

    // Build a provider from the pool wallet's signer (local key or KMS, depending on
    // deployment) to send the transfer below.
    let funding_provider = wallet_handle
        .build_provider(&state.provider.rpc_url)
        .map_err(|e| {
            let detailed_error = format!("Failed to build funding provider: {e}");
            tracing::error!("{}", detailed_error);
            (
                Status::InternalServerError,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: "Server RPC configuration is invalid".to_string(),
                }),
            )
        })?;

    // Confirm the lock is still held immediately before submitting.
    if let Err(e) = wallet_handle.ensure_lock_held() {
        let detailed_error = format!("Pool wallet lock lost before USDC transfer: {e}");
        tracing::error!("{}", detailed_error);
        return Err((
            Status::InternalServerError,
            Json(ApiResponse {
                success: false,
                data: None,
                message: format!("USDC transfer aborted: {e}"),
            }),
        ));
    }

    // Send USDC using funding provider.
    let usdc_send_contract = IERC20::new(state.contracts.usdc, &funding_provider);
    let usdc_receipt = match usdc_send_contract
        .transfer(wallet_address, U256::from(usdc_amount))
        .send()
        .await
    {
        Ok(pending) => {
            let usdc_tx_hash = *pending.tx_hash();
            match timeout(FUNDING_RECEIPT_TIMEOUT, pending.get_receipt()).await {
                // A receipt can come back for a REVERTED transfer — accepting it
                // would report a successful payout when no USDC moved. Treat a
                // non-success status as a failure (no funds moved on a revert, so
                // it is safe to retry once the cause is understood).
                Ok(Ok(receipt)) if !receipt.status() => {
                    let detailed_error =
                        format!("USDC transfer reverted on-chain (tx {usdc_tx_hash:?})");
                    tracing::error!("{}", detailed_error);
                    return Err((
                        Status::InternalServerError,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            message: format!(
                                "USDC transfer reverted on-chain (tx {usdc_tx_hash:?}); \
                                 no USDC moved"
                            ),
                        }),
                    ));
                }
                Ok(Ok(receipt)) => receipt,
                Ok(Err(e)) => {
                    let detailed_error = format!("Failed to get USDC transaction receipt: {e}");
                    tracing::error!("{}", detailed_error);
                    return Err((
                        Status::InternalServerError,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            message: format!(
                                "USDC transfer sent (tx {usdc_tx_hash:?}) but confirmation \
                                 failed — verify on-chain before retrying to avoid double-funding"
                            ),
                        }),
                    ));
                }
                Err(_) => {
                    let detailed_error = format!(
                        "Timeout waiting for USDC transfer receipt (tx {usdc_tx_hash:?}) after {}s",
                        FUNDING_RECEIPT_TIMEOUT.as_secs()
                    );
                    tracing::error!("{}", detailed_error);
                    return Err((
                        Status::InternalServerError,
                        Json(ApiResponse {
                            success: false,
                            data: None,
                            message: format!(
                                "USDC transfer unconfirmed after {}s (tx {usdc_tx_hash:?}) — \
                                 verify on-chain before retrying to avoid double-funding",
                                FUNDING_RECEIPT_TIMEOUT.as_secs()
                            ),
                        }),
                    ));
                }
            }
        }
        Err(e) => {
            let detailed_error = format!("Failed to send USDC: {e}");
            tracing::error!("{}", detailed_error);
            return Err((
                Status::InternalServerError,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: "Failed to send USDC".to_string(),
                }),
            ));
        }
    };

    tracing::info!(
        "Bonus USDC transfer hash: {:?}",
        usdc_receipt.transaction_hash
    );

    // `data` is the transaction hash itself (not a sentence) so callers can
    // consume it directly without parsing prose. The human-readable summary
    // lives in `message`.
    Ok(Json(ApiResponse {
        success: true,
        data: Some(usdc_receipt.transaction_hash.to_string()),
        message: format!(
            "Successfully funded wallet {wallet_address} with {} USDC",
            usdc_amount / 1_000_000
        ),
    }))
}

/// Tops up pool wallets with testnet USDC by minting (admin, testnet-only).
///
/// The deployed testnet USDC has a permissionless `mint`, so the pool can
/// replenish its own USDC: every pool wallet below the per-wallet target is
/// minted up to it. ETH cannot be minted — gas top-ups remain a manual
/// operation (see the runbook in README.md).
#[openapi(tag = "Wallet")]
#[post("/top_up_pool", format = "json", data = "<request>")]
pub async fn top_up_pool(
    state: &State<AppState>,
    request: Json<TopUpPoolRequest>,
    _token: AdminToken,
) -> Result<Json<ApiResponse<Vec<String>>>, (Status, Json<ApiResponse<Vec<String>>>)> {
    tracing::info!("Received request: POST /top_up_pool");

    // Same fail-closed production guard as fund_guest_wallet: minting play
    // money only exists on Arbitrum Sepolia / local Anvil.
    if is_production_chain(state.provider.chain_id) {
        let error_msg = format!(
            "top_up_pool is disabled on chain id {} (production network); \
             this endpoint only runs on Arbitrum Sepolia / local Anvil",
            state.provider.chain_id
        );
        tracing::error!("{}", error_msg);
        return Err((
            Status::Forbidden,
            Json(ApiResponse {
                success: false,
                data: None,
                message: error_msg,
            }),
        ));
    }

    let usdc_target = match &request.usdc_target {
        None => DEFAULT_TOP_UP_USDC_TARGET,
        Some(raw) => match raw.parse::<u128>() {
            Ok(v) if v > 0 => v,
            Ok(_) | Err(_) => {
                return Err((
                    Status::BadRequest,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        message: format!("Invalid usdc_target: {raw:?}"),
                    }),
                ));
            }
        },
    };

    let pool_addresses = state.wallets.manager.signer_addresses();
    if pool_addresses.is_empty() {
        return Err((
            Status::ServiceUnavailable,
            Json(ApiResponse {
                success: false,
                data: None,
                message: "Wallet pool is empty".to_string(),
            }),
        ));
    }

    // Determine deficits from fresh on-chain balances.
    let usdc_read_contract = IERC20::new(state.contracts.usdc, &*state.provider.read_provider);
    let mut deficits: Vec<(Address, U256)> = Vec::new();
    for &wallet in &pool_addresses {
        let balance = match usdc_read_contract.balanceOf(wallet).call().await {
            Ok(balance) => balance,
            Err(e) => {
                tracing::warn!("top_up_pool: failed to read USDC balance for {wallet}: {e}");
                continue;
            }
        };
        if balance < U256::from(usdc_target) {
            deficits.push((wallet, U256::from(usdc_target) - balance));
        }
    }

    if deficits.is_empty() {
        return Ok(Json(ApiResponse {
            success: true,
            data: Some(vec![]),
            message: format!(
                "All {} pool wallets already at or above the {} USDC target",
                pool_addresses.len(),
                usdc_target / 1_000_000
            ),
        }));
    }

    // Any pool wallet can send the mints (the mint is permissionless); it
    // only needs gas. Acquire one through the manager so the sends don't
    // race a concurrent funding request on the same nonce.
    let minter_handle = state
        .wallets
        .manager
        .acquire_wallet_for_usdc(U256::ZERO, &std::collections::HashSet::new())
        .await
        .map_err(|e| {
            let detailed_error = format!("Failed to acquire minter wallet: {e}");
            tracing::error!("{}", detailed_error);
            (
                Status::ServiceUnavailable,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: "Minter wallet temporarily unavailable".to_string(),
                }),
            )
        })?;

    let minter_provider = minter_handle
        .build_provider(&state.provider.rpc_url)
        .map_err(|e| {
            let detailed_error = format!("Failed to build minter provider: {e}");
            tracing::error!("{}", detailed_error);
            (
                Status::InternalServerError,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: "Server RPC configuration is invalid".to_string(),
                }),
            )
        })?;

    let usdc_mint_contract = ITestnetUSDC::new(state.contracts.usdc, &minter_provider);
    let mut results: Vec<String> = Vec::new();
    let mut failures = 0usize;

    for (wallet, deficit) in &deficits {
        if let Err(e) = minter_handle.ensure_lock_held() {
            tracing::error!("top_up_pool: minter wallet lock lost mid-run: {e}");
            results.push(format!("{wallet}: skipped (minter lock lost)"));
            failures += 1;
            continue;
        }

        match usdc_mint_contract.mint(*wallet, *deficit).send().await {
            Ok(pending) => {
                let tx_hash = *pending.tx_hash();
                match timeout(FUNDING_RECEIPT_TIMEOUT, pending.get_receipt()).await {
                    Ok(Ok(receipt)) if receipt.status() => {
                        tracing::info!(
                            "top_up_pool: minted {} USDC to {} (tx {:?})",
                            deficit / U256::from(1_000_000),
                            wallet,
                            receipt.transaction_hash
                        );
                        results.push(format!(
                            "{wallet}: minted {} USDC (tx {:?})",
                            deficit / U256::from(1_000_000),
                            receipt.transaction_hash
                        ));
                    }
                    Ok(Ok(_)) => {
                        tracing::error!("top_up_pool: mint reverted for {wallet} (tx {tx_hash:?})");
                        results.push(format!("{wallet}: mint reverted (tx {tx_hash:?})"));
                        failures += 1;
                    }
                    Ok(Err(e)) => {
                        tracing::error!("top_up_pool: mint receipt failed for {wallet}: {e}");
                        results.push(format!("{wallet}: mint unconfirmed (tx {tx_hash:?})"));
                        failures += 1;
                    }
                    Err(_) => {
                        tracing::error!("top_up_pool: mint receipt timeout for {wallet}");
                        results.push(format!("{wallet}: mint unconfirmed (tx {tx_hash:?})"));
                        failures += 1;
                    }
                }
            }
            Err(e) => {
                tracing::error!("top_up_pool: mint send failed for {wallet}: {e}");
                results.push(format!("{wallet}: mint send failed"));
                failures += 1;
            }
        }
    }

    let message = format!(
        "Topped up {}/{} wallets to the {} USDC target",
        deficits.len() - failures,
        deficits.len(),
        usdc_target / 1_000_000
    );

    Ok(Json(ApiResponse {
        success: failures == 0,
        data: Some(results),
        message,
    }))
}

// Tests moved to tests/integration_tests/wallet_test.rs
