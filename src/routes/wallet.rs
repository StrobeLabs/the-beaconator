use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use alloy::sol;
use rocket::serde::json::Json;
use rocket::{State, http::Status, post};
use std::str::FromStr;
use tracing;

use crate::guards::ApiToken;
use crate::models::{ApiResponse, AppState, FundGuestWalletRequest};

// Define ERC20 interface for USDC transfers
sol! {
    #[sol(rpc)]
    interface IERC20 {
        function transfer(address to, uint256 amount) external returns (bool);
        function balanceOf(address account) external view returns (uint256 balance);
    }
}

#[post("/fund_guest_wallet", format = "json", data = "<request>")]
pub async fn fund_guest_wallet(
    state: &State<AppState>,
    request: Json<FundGuestWalletRequest>,
    _token: ApiToken,
) -> Result<Json<ApiResponse<String>>, (Status, Json<ApiResponse<String>>)> {
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
        Ok(pending) => match pending.watch().await {
            Ok(hash) => hash,
            Err(e) => {
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

#[cfg(test)]
#[path = "wallet_test.rs"]
mod wallet_test;
