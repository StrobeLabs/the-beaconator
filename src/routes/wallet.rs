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

use super::{IERC20, sentry_error};
use crate::guards::ApiToken;
use crate::models::{ApiResponse, AppState, FundBonusWalletRequest, FundGuestWalletRequest};

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
    let hub = sentry::Hub::new_from_top(sentry::Hub::main());
    hub.add_breadcrumb(sentry::Breadcrumb {
        ty: "http".into(),
        category: Some("request".into()),
        message: Some(format!(
            "POST /fund_guest_wallet wallet={}",
            request.wallet_address
        )),
        ..Default::default()
    });
    hub.configure_scope(|scope| {
        scope.set_tag("endpoint", "/fund_guest_wallet");
        scope.set_extra("wallet_address", request.wallet_address.clone().into());
        scope.set_extra("usdc_amount", request.usdc_amount.clone().into());
        scope.set_extra("eth_amount", request.eth_amount.clone().into());
    });

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
        sentry_error(
            &hub,
            "ProductionGuardrail",
            error_msg.clone(),
            vec![("chain_id", state.provider.chain_id.to_string().into())],
        );
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

    // Acquire a pool wallet up front — before any balance checks — so the ETH/USDC
    // balances we check are the ones that will actually fund the transfer. The
    // measurement signer (PRIVATE_KEY) never sends funds; all sends go through the
    // KMS-capable pool. WalletHandle already carries the distributed lock plus a
    // background heartbeat that extends it, so no separate lock/heartbeat management
    // is needed here — the wallet stays reserved until `wallet_handle` drops.
    let wallet_handle = state
        .wallets
        .manager
        .acquire_any_wallet()
        .await
        .map_err(|e| {
            let detailed_error = format!("Failed to acquire pool wallet: {e}");
            tracing::error!("{}", detailed_error);
            sentry_error(&hub, "WalletError", detailed_error, vec![]);
            (
                Status::ServiceUnavailable,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: "Funding wallet temporarily unavailable".to_string(),
                }),
            )
        })?;

    // Check pool wallet ETH balance using read provider
    let eth_balance = match state
        .provider
        .read_provider
        .get_balance(wallet_handle.address())
        .await
    {
        Ok(balance) => balance,
        Err(e) => {
            let detailed_error = format!("Failed to get ETH balance: {e}");
            tracing::error!("{}", detailed_error);
            sentry_error(&hub, "RpcError", detailed_error, vec![]);
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

    // Check if we have enough ETH
    if eth_balance < U256::from(eth_amount) {
        tracing::warn!(
            "Insufficient ETH balance in pool wallet {}. Have: {} ETH, Need: {} ETH",
            wallet_handle.address(),
            alloy::primitives::utils::format_ether(eth_balance),
            alloy::primitives::utils::format_ether(U256::from(eth_amount))
        );
        hub.capture_message(
            &format!(
                "Insufficient ETH balance in pool wallet. Have: {} ETH, Need: {} ETH",
                alloy::primitives::utils::format_ether(eth_balance),
                alloy::primitives::utils::format_ether(U256::from(eth_amount))
            ),
            sentry::Level::Warning,
        );
        return Err((
            Status::InternalServerError,
            Json(ApiResponse {
                success: false,
                data: None,
                message: format!(
                    "Insufficient ETH balance. Have: {} ETH, Need: {} ETH",
                    alloy::primitives::utils::format_ether(eth_balance),
                    alloy::primitives::utils::format_ether(U256::from(eth_amount))
                ),
            }),
        ));
    }

    // Check USDC balance using read provider
    let usdc_read_contract = IERC20::new(state.contracts.usdc, &*state.provider.read_provider);
    let usdc_balance = match usdc_read_contract
        .balanceOf(wallet_handle.address())
        .call()
        .await
    {
        Ok(result) => result,
        Err(e) => {
            let detailed_error = format!("Failed to get USDC balance: {e}");
            tracing::error!("{}", detailed_error);
            sentry_error(&hub, "RpcError", detailed_error, vec![]);
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
            wallet_handle.address(),
            usdc_balance / U256::from(1_000_000),
            usdc_amount / 1_000_000
        );
        hub.capture_message(
            &format!(
                "Insufficient USDC balance in pool wallet. Have: {} USDC, Need: {} USDC",
                usdc_balance / U256::from(1_000_000),
                usdc_amount / 1_000_000
            ),
            sentry::Level::Warning,
        );
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

    // Build a provider from the pool wallet's signer (local key or KMS, depending on
    // deployment) to send the two on-chain transfers below.
    let funding_provider = wallet_handle
        .build_provider(&state.provider.rpc_url)
        .map_err(|e| {
            let detailed_error = format!("Failed to build funding provider: {e}");
            tracing::error!("{}", detailed_error);
            sentry_error(&hub, "ConfigError", detailed_error, vec![]);
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
                    sentry_error(&hub, "TransactionError", detailed_error, vec![]);
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
                    sentry_error(&hub, "TransactionError", detailed_error, vec![]);
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
            sentry_error(&hub, "TransactionError", detailed_error, vec![]);
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
        sentry_error(&hub, "WalletError", detailed_error, vec![]);
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
                    sentry_error(&hub, "TransactionError", detailed_error, vec![]);
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
                    sentry_error(&hub, "TransactionError", detailed_error, vec![]);
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
            sentry_error(&hub, "TransactionError", detailed_error, vec![]);
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
    let hub = sentry::Hub::new_from_top(sentry::Hub::main());
    hub.add_breadcrumb(sentry::Breadcrumb {
        ty: "http".into(),
        category: Some("request".into()),
        message: Some(format!(
            "POST /fund_bonus_wallet wallet={}",
            request.wallet_address
        )),
        ..Default::default()
    });
    hub.configure_scope(|scope| {
        scope.set_tag("endpoint", "/fund_bonus_wallet");
        scope.set_extra("wallet_address", request.wallet_address.clone().into());
        scope.set_extra("usdc_amount", request.usdc_amount.clone().into());
    });

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

    // Acquire a pool wallet up front — before the balance check — so the USDC balance
    // we check is the one that will actually fund the transfer. The measurement signer
    // (PRIVATE_KEY) never sends funds; all sends go through the KMS-capable pool.
    // WalletHandle already carries the distributed lock plus a background heartbeat
    // that extends it, so no separate lock/heartbeat management is needed here.
    let wallet_handle = state
        .wallets
        .manager
        .acquire_any_wallet()
        .await
        .map_err(|e| {
            let detailed_error = format!("Failed to acquire pool wallet: {e}");
            tracing::error!("{}", detailed_error);
            sentry_error(&hub, "WalletError", detailed_error, vec![]);
            (
                Status::ServiceUnavailable,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: "Funding wallet temporarily unavailable".to_string(),
                }),
            )
        })?;

    // Check pool wallet USDC balance using read provider
    let usdc_read_contract = IERC20::new(state.contracts.usdc, &*state.provider.read_provider);
    let usdc_balance = match usdc_read_contract
        .balanceOf(wallet_handle.address())
        .call()
        .await
    {
        Ok(result) => result,
        Err(e) => {
            let detailed_error = format!("Failed to get USDC balance: {e}");
            tracing::error!("{}", detailed_error);
            sentry_error(&hub, "RpcError", detailed_error, vec![]);
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
            wallet_handle.address(),
            usdc_balance / U256::from(1_000_000),
            usdc_amount / 1_000_000
        );
        hub.capture_message(
            &format!(
                "Insufficient USDC balance in bonus pool wallet. Have: {} USDC, Need: {} USDC",
                usdc_balance / U256::from(1_000_000),
                usdc_amount / 1_000_000
            ),
            sentry::Level::Warning,
        );
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

    // Build a provider from the pool wallet's signer (local key or KMS, depending on
    // deployment) to send the transfer below.
    let funding_provider = wallet_handle
        .build_provider(&state.provider.rpc_url)
        .map_err(|e| {
            let detailed_error = format!("Failed to build funding provider: {e}");
            tracing::error!("{}", detailed_error);
            sentry_error(&hub, "ConfigError", detailed_error, vec![]);
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
        sentry_error(&hub, "WalletError", detailed_error, vec![]);
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
                    sentry_error(&hub, "TransactionError", detailed_error, vec![]);
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
                    sentry_error(&hub, "TransactionError", detailed_error, vec![]);
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
                    sentry_error(&hub, "TransactionError", detailed_error, vec![]);
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
            sentry_error(&hub, "TransactionError", detailed_error, vec![]);
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

// Tests moved to tests/integration_tests/wallet_test.rs
