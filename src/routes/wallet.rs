use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use rocket::serde::json::Json;
use rocket::{State, http::Status, post};
use std::str::FromStr;
use tracing;

use super::IERC20;
use crate::guards::ApiToken;
use crate::models::{ApiResponse, AppState, FundGuestWalletRequest};

/// Funds a guest wallet with USDC and ETH.
///
/// Transfers the specified amounts of USDC and ETH from the beaconator wallet
/// to the guest wallet address. Validates transfer limits and available balances.
#[post("/fund_guest_wallet", format = "json", data = "<request>")]
pub async fn fund_guest_wallet(
    state: &State<AppState>,
    request: Json<FundGuestWalletRequest>,
    _token: ApiToken,
) -> Result<Json<ApiResponse<String>>, (Status, Json<ApiResponse<String>>)> {
    tracing::info!("Received request: POST /fund_guest_wallet");
    let _guard = sentry::Hub::current().push_scope();
    sentry::configure_scope(|scope| {
        scope.set_tag("endpoint", "/fund_guest_wallet");
        scope.set_extra("wallet_address", request.wallet_address.clone().into());
        scope.set_extra("usdc_amount", request.usdc_amount.clone().into());
        scope.set_extra("eth_amount", request.eth_amount.clone().into());
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
    if usdc_amount > state.usdc_transfer_limit {
        return Err((
            Status::BadRequest,
            Json(ApiResponse {
                success: false,
                data: None,
                message: format!(
                    "USDC amount exceeds limit. Requested: {} USDC, Limit: {} USDC",
                    usdc_amount / 1_000_000,
                    state.usdc_transfer_limit / 1_000_000
                ),
            }),
        ));
    }

    if eth_amount > state.eth_transfer_limit {
        return Err((
            Status::BadRequest,
            Json(ApiResponse {
                success: false,
                data: None,
                message: format!(
                    "ETH amount exceeds limit. Requested: {} ETH, Limit: {} ETH",
                    alloy::primitives::utils::format_ether(U256::from(eth_amount)),
                    alloy::primitives::utils::format_ether(U256::from(state.eth_transfer_limit))
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

    // Check beaconator wallet ETH balance
    let eth_balance = match state.provider.get_balance(state.wallet_address).await {
        Ok(balance) => balance,
        Err(e) => {
            tracing::error!("Failed to get ETH balance: {}", e);
            sentry::capture_message(
                &format!("Failed to get ETH balance: {e}"),
                sentry::Level::Error,
            );
            return Err((
                Status::InternalServerError,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!("Failed to get ETH balance: {e}"),
                }),
            ));
        }
    };

    // Check if we have enough ETH
    if eth_balance < U256::from(eth_amount) {
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

    // Check USDC balance
    let usdc_contract = IERC20::new(state.usdc_address, &*state.provider);
    let usdc_balance = match usdc_contract.balanceOf(state.wallet_address).call().await {
        Ok(result) => result,
        Err(e) => {
            tracing::error!("Failed to get USDC balance: {}", e);
            sentry::capture_message(
                &format!("Failed to get USDC balance: {e}"),
                sentry::Level::Error,
            );
            return Err((
                Status::InternalServerError,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!("Failed to get USDC balance: {e}"),
                }),
            ));
        }
    };

    // Check if we have enough USDC
    if usdc_balance < U256::from(usdc_amount) {
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

    // Send ETH
    let tx_request = TransactionRequest::default()
        .to(wallet_address)
        .value(U256::from(eth_amount));

    let eth_tx_hash = match state.provider.send_transaction(tx_request).await {
        Ok(pending) => match pending.get_receipt().await {
            Ok(receipt) => receipt.transaction_hash,
            Err(e) => {
                tracing::error!("Failed to get ETH transaction receipt: {}", e);
                sentry::capture_message(
                    &format!("Failed to get ETH transaction receipt: {e}"),
                    sentry::Level::Error,
                );
                return Err((
                    Status::InternalServerError,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        message: format!("Failed to confirm ETH transaction: {e}"),
                    }),
                ));
            }
        },
        Err(e) => {
            tracing::error!("Failed to send ETH: {}", e);
            sentry::capture_message(&format!("Failed to send ETH: {e}"), sentry::Level::Error);
            return Err((
                Status::InternalServerError,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!("Failed to send ETH: {e}"),
                }),
            ));
        }
    };

    tracing::info!("ETH transfer hash: {:?}", eth_tx_hash);

    // Send USDC
    let usdc_receipt = match usdc_contract
        .transfer(wallet_address, U256::from(usdc_amount))
        .send()
        .await
    {
        Ok(pending) => match pending.get_receipt().await {
            Ok(receipt) => receipt,
            Err(e) => {
                tracing::error!("Failed to get USDC transaction receipt: {}", e);
                sentry::capture_message(
                    &format!("Failed to get USDC transaction receipt: {e}"),
                    sentry::Level::Error,
                );
                return Err((
                    Status::InternalServerError,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        message: format!("Failed to get USDC transaction receipt: {e}"),
                    }),
                ));
            }
        },
        Err(e) => {
            tracing::error!("Failed to send USDC: {}", e);
            sentry::capture_message(&format!("Failed to send USDC: {e}"), sentry::Level::Error);
            return Err((
                Status::InternalServerError,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    message: format!("Failed to send USDC: {e}"),
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

// Tests moved to tests/integration_tests/wallet_test.rs
