#[cfg(test)]
mod tests {
    use the_beaconator::models::FundGuestWalletRequest;
    use the_beaconator::routes::IERC20;
    use the_beaconator::routes::wallet::fund_guest_wallet;
    // test_utils imports - currently unused but available for future tests
    // use crate::test_utils::{TestUtils, create_test_app_state};
    use crate::test_utils::{TestUtils, create_isolated_test_app_state};
    use alloy::primitives::Address;
    use rocket::serde::json::Json;
    use rocket::{State, http::Status};
    use serial_test::serial;
    use std::str::FromStr;

    #[tokio::test]
    #[serial]
    #[ignore] // Temporarily disabled - hangs due to real network calls
    async fn test_fund_guest_wallet_invalid_address() {
        let (app_state, _anvil) = create_isolated_test_app_state().await;
        let state = State::from(&app_state);

        let request = Json(FundGuestWalletRequest {
            wallet_address: "invalid_address".to_string(),
            usdc_amount: "100000000".to_string(), // 100 USDC
            eth_amount: "1000000000000000".to_string(), // 0.001 ETH
        });

        let result = fund_guest_wallet(
            state,
            request,
            the_beaconator::guards::ApiToken("test_token".to_string()),
        )
        .await;

        assert!(result.is_err());
        let (status, response) = result.unwrap_err();
        assert_eq!(status, Status::BadRequest);
        assert!(!response.success);
        assert!(response.message.contains("Invalid wallet address"));
    }

    #[tokio::test]
    #[serial]
    #[ignore] // Temporarily disabled - hangs due to real network calls
    async fn test_fund_guest_wallet_insufficient_balance() {
        let (app_state, _anvil) = create_isolated_test_app_state().await;
        let state = State::from(&app_state);

        // Use a valid address
        let guest_address =
            Address::from_str("0x742d35Cc6634C0532925a3b844Bc9e7595f8b94b").unwrap();

        let request = Json(FundGuestWalletRequest {
            wallet_address: guest_address.to_string(),
            usdc_amount: "100000000".to_string(), // 100 USDC
            eth_amount: "1000000000000000".to_string(), // 0.001 ETH
        });

        // In a real test environment without actual funds, this should fail
        let result = fund_guest_wallet(
            state,
            request,
            the_beaconator::guards::ApiToken("test_token".to_string()),
        )
        .await;

        // We expect this to fail due to insufficient balance in test environment
        assert!(result.is_err());
        let (status, response) = result.unwrap_err();
        assert_eq!(status, Status::InternalServerError);
        assert!(!response.success);
        // The error could be either insufficient ETH or USDC balance, or network error
        assert!(
            response.message.contains("Insufficient")
                || response.message.contains("Failed to get")
                || response.message.contains("Failed to send")
        );
    }

    #[tokio::test]
    #[serial]
    #[ignore] // Temporarily disabled - hangs due to real network calls
    async fn test_fund_guest_wallet_exceeds_limits() {
        let (app_state, _anvil) = create_isolated_test_app_state().await;
        let state = State::from(&app_state);

        let guest_address =
            Address::from_str("0x742d35Cc6634C0532925a3b844Bc9e7595f8b94b").unwrap();

        // Test USDC limit exceeded
        let request = Json(FundGuestWalletRequest {
            wallet_address: guest_address.to_string(),
            usdc_amount: "2000000000".to_string(), // 2000 USDC (exceeds default 1000 limit)
            eth_amount: "1000000000000000".to_string(), // 0.001 ETH
        });

        let result = fund_guest_wallet(
            state,
            request,
            the_beaconator::guards::ApiToken("test_token".to_string()),
        )
        .await;

        assert!(result.is_err());
        let (status, response) = result.unwrap_err();
        assert_eq!(status, Status::BadRequest);
        assert!(!response.success);
        assert!(response.message.contains("USDC amount exceeds limit"));

        // Test ETH limit exceeded
        let request = Json(FundGuestWalletRequest {
            wallet_address: guest_address.to_string(),
            usdc_amount: "100000000".to_string(), // 100 USDC
            eth_amount: "20000000000000000".to_string(), // 0.02 ETH (exceeds default 0.01 limit)
        });

        let result = fund_guest_wallet(
            state,
            request,
            the_beaconator::guards::ApiToken("test_token".to_string()),
        )
        .await;

        assert!(result.is_err());
        let (status, response) = result.unwrap_err();
        assert_eq!(status, Status::BadRequest);
        assert!(!response.success);
        assert!(response.message.contains("ETH amount exceeds limit"));
    }

    #[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
    #[serial]
    async fn test_fund_guest_wallet_invalid_amounts() {
        let (app_state, _anvil) = create_isolated_test_app_state().await;
        let state = State::from(&app_state);

        let guest_address =
            Address::from_str("0x742d35Cc6634C0532925a3b844Bc9e7595f8b94b").unwrap();

        // Test invalid USDC amount
        let request = Json(FundGuestWalletRequest {
            wallet_address: guest_address.to_string(),
            usdc_amount: "not_a_number".to_string(),
            eth_amount: "1000000000000000".to_string(),
        });

        let result = fund_guest_wallet(
            state,
            request,
            the_beaconator::guards::ApiToken("test_token".to_string()),
        )
        .await;

        assert!(result.is_err());
        let (status, response) = result.unwrap_err();
        assert_eq!(status, Status::BadRequest);
        assert!(!response.success);
        assert!(response.message.contains("Invalid USDC amount"));
    }

    #[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
    #[serial]
    async fn test_fund_guest_wallet_success_scenario() {
        // This test would require a properly funded test environment
        // For unit tests, we're focusing on error handling and validation

        let (app_state, _anvil) = create_isolated_test_app_state().await;

        // Verify test setup
        assert_ne!(app_state.wallet_address, Address::ZERO);
        assert_ne!(app_state.usdc_address, Address::ZERO);

        // Check that we can get the balance (even if it's zero)
        let balance_result =
            TestUtils::get_balance(&app_state.provider, app_state.wallet_address).await;
        assert!(balance_result.is_ok());
    }

    #[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
    #[serial]
    async fn test_ierc20_interface() {
        // Test that IERC20 interface is properly defined
        let (app_state, _anvil) = create_isolated_test_app_state().await;
        let usdc_contract = IERC20::new(app_state.usdc_address, &*app_state.provider);

        // Verify the contract instance was created
        assert_eq!(*usdc_contract.address(), app_state.usdc_address);
    }

    #[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
    #[serial]
    async fn test_fund_guest_wallet_zero_amounts() {
        let (app_state, _anvil) = create_isolated_test_app_state().await;
        let state = State::from(&app_state);

        let guest_address =
            Address::from_str("0x742d35Cc6634C0532925a3b844Bc9e7595f8b94b").unwrap();

        // Test with zero amounts
        let request = Json(FundGuestWalletRequest {
            wallet_address: guest_address.to_string(),
            usdc_amount: "0".to_string(),
            eth_amount: "0".to_string(),
        });

        let result = fund_guest_wallet(
            state,
            request,
            the_beaconator::guards::ApiToken("test_token".to_string()),
        )
        .await;

        // Should fail due to network issues, not because of zero amounts
        assert!(result.is_err());
        let (status, response) = result.unwrap_err();
        assert_eq!(status, Status::InternalServerError);
        assert!(!response.success);
    }

    #[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
    #[serial]
    async fn test_fund_guest_wallet_negative_amounts() {
        let (app_state, _anvil) = create_isolated_test_app_state().await;
        let state = State::from(&app_state);

        let guest_address =
            Address::from_str("0x742d35Cc6634C0532925a3b844Bc9e7595f8b94b").unwrap();

        // Test with negative amounts (should fail parsing)
        let request = Json(FundGuestWalletRequest {
            wallet_address: guest_address.to_string(),
            usdc_amount: "-1000000".to_string(),
            eth_amount: "1000000000000000".to_string(),
        });

        let result = fund_guest_wallet(
            state,
            request,
            the_beaconator::guards::ApiToken("test_token".to_string()),
        )
        .await;

        assert!(result.is_err());
        let (status, response) = result.unwrap_err();
        assert_eq!(status, Status::BadRequest);
        assert!(!response.success);
        assert!(response.message.contains("Invalid USDC amount"));
    }

    #[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
    #[serial]
    async fn test_fund_guest_wallet_eth_limit_exceeded() {
        let (app_state, _anvil) = create_isolated_test_app_state().await;
        let state = State::from(&app_state);

        let guest_address =
            Address::from_str("0x742d35Cc6634C0532925a3b844Bc9e7595f8b94b").unwrap();

        // Test ETH limit exceeded (default limit is 0.01 ETH)
        let request = Json(FundGuestWalletRequest {
            wallet_address: guest_address.to_string(),
            usdc_amount: "1000000".to_string(),          // 1 USDC
            eth_amount: "20000000000000000".to_string(), // 0.02 ETH (exceeds default 0.01 limit)
        });

        let result = fund_guest_wallet(
            state,
            request,
            the_beaconator::guards::ApiToken("test_token".to_string()),
        )
        .await;

        assert!(result.is_err());
        let (status, response) = result.unwrap_err();
        assert_eq!(status, Status::BadRequest);
        assert!(!response.success);
        assert!(response.message.contains("ETH amount exceeds limit"));
    }

    #[tokio::test]
    #[ignore] // Temporarily disabled - hangs due to real network calls
    #[serial]
    async fn test_fund_guest_wallet_invalid_amount_format() {
        let (app_state, _anvil) = create_isolated_test_app_state().await;
        let state = State::from(&app_state);

        let guest_address =
            Address::from_str("0x742d35Cc6634C0532925a3b844Bc9e7595f8b94b").unwrap();

        // Test with invalid USDC amount format
        let request = Json(FundGuestWalletRequest {
            wallet_address: guest_address.to_string(),
            usdc_amount: "not_a_number".to_string(),
            eth_amount: "1000000000000000".to_string(),
        });

        let result = fund_guest_wallet(
            state,
            request,
            the_beaconator::guards::ApiToken("test_token".to_string()),
        )
        .await;

        assert!(result.is_err());
        let (status, response) = result.unwrap_err();
        assert_eq!(status, Status::BadRequest);
        assert!(!response.success);
        assert!(response.message.contains("Invalid USDC amount"));

        // Test with invalid ETH amount format
        let request2 = Json(FundGuestWalletRequest {
            wallet_address: guest_address.to_string(),
            usdc_amount: "1000000".to_string(),
            eth_amount: "not_a_number".to_string(),
        });

        let result2 = fund_guest_wallet(
            state,
            request2,
            the_beaconator::guards::ApiToken("test_token".to_string()),
        )
        .await;

        assert!(result2.is_err());
        let (status2, response2) = result2.unwrap_err();
        assert_eq!(status2, Status::BadRequest);
        assert!(!response2.success);
        assert!(response2.message.contains("Invalid ETH amount"));
    }
}
