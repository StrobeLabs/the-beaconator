// Comprehensive unit tests for wallet routes

use alloy::primitives::Address;
use rocket::serde::json::Json;
use rocket::{State, http::Status};
use std::str::FromStr;
use the_beaconator::guards::ApiToken;
use the_beaconator::models::FundGuestWalletRequest;
use the_beaconator::routes::wallet::fund_guest_wallet;

// Helper to create test app state
fn create_test_state() -> the_beaconator::models::AppState {
    crate::test_utils::create_simple_test_app_state()
}

#[tokio::test]
async fn test_fund_wallet_invalid_address() {
    let test_state = create_test_state();
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "invalid_address".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(&state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_empty_address() {
    let test_state = create_test_state();
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(&state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_invalid_usdc_amount() {
    let test_state = create_test_state();
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "not_a_number".to_string(),
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(&state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_invalid_eth_amount() {
    let test_state = create_test_state();
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "not_a_number".to_string(),
    });

    let result = fund_guest_wallet(&state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_negative_usdc() {
    let test_state = create_test_state();
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "-1000000".to_string(),
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(&state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_negative_eth() {
    let test_state = create_test_state();
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "-1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(&state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_usdc_exceeds_limit() {
    let mut state = create_test_state();
    state.usdc_transfer_limit = 10_000_000; // 10 USDC
    let state = State::from(&state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "20000000".to_string(), // 20 USDC
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(&state, request, token).await;
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
    assert!(response.into_inner().message.contains("exceeds limit"));
}

#[tokio::test]
async fn test_fund_wallet_eth_exceeds_limit() {
    let mut state = create_test_state();
    state.eth_transfer_limit = 1_000_000_000_000_000; // 0.001 ETH
    let state = State::from(&state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "2000000000000000".to_string(), // 0.002 ETH
    });

    let result = fund_guest_wallet(&state, request, token).await;
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
    assert!(response.into_inner().message.contains("exceeds limit"));
}

#[tokio::test]
async fn test_fund_wallet_zero_amounts() {
    let test_state = create_test_state();
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "0".to_string(),
        eth_amount: "0".to_string(),
    });

    // Zero amounts are technically valid, but will fail at network level
    let result = fund_guest_wallet(&state, request, token).await;
    // Could be BadRequest or InternalServerError depending on validation
    assert!(result.is_err());
}

#[tokio::test]
async fn test_fund_wallet_valid_format_network_failure() {
    let test_state = create_test_state();
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "1000000000000000".to_string(),
    });

    // Valid input but should fail due to network issues in test environment
    let result = fund_guest_wallet(&state, request, token).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_fund_wallet_decimal_usdc_amount() {
    let test_state = create_test_state();
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "10.5".to_string(), // Decimals not allowed
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(&state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_scientific_notation() {
    let test_state = create_test_state();
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "1e6".to_string(), // Scientific notation
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(&state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_address_with_mixed_case() {
    let test_state = create_test_state();
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    // Mixed case checksum address
    let request = Json(FundGuestWalletRequest {
        wallet_address: "0xAbCdEf1234567890123456789012345678901234".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "1000000000000000".to_string(),
    });

    // Should parse correctly but fail at network level
    let result = fund_guest_wallet(&state, request, token).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_fund_wallet_max_u128_amounts() {
    let test_state = create_test_state();
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: u128::MAX.to_string(),
        eth_amount: u128::MAX.to_string(),
    });

    // Should fail due to exceeding limits
    let result = fund_guest_wallet(&state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[test]
fn test_address_parsing_edge_cases() {
    // Zero address
    let zero_addr = Address::from_str("0x0000000000000000000000000000000000000000");
    assert!(zero_addr.is_ok());

    // Max address
    let max_addr = Address::from_str("0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF");
    assert!(max_addr.is_ok());

    // Too long (41 characters)
    let no_prefix = Address::from_str("12345678901234567890123456789012345678901");
    assert!(no_prefix.is_err());
}

#[test]
fn test_amount_parsing_edge_cases() {
    // Test boundary values
    assert!("0".parse::<u128>().is_ok());
    assert!("1".parse::<u128>().is_ok());
    assert!(u128::MAX.to_string().parse::<u128>().is_ok());

    // Test overflow
    let overflow = format!("{}0", u128::MAX);
    assert!(overflow.parse::<u128>().is_err());
}
