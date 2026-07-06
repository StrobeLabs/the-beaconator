// Comprehensive unit tests for wallet routes

use alloy::primitives::Address;
use rocket::serde::json::Json;
use rocket::{State, http::Status};
use std::str::FromStr;
use the_beaconator::guards::ApiToken;
use the_beaconator::models::FundGuestWalletRequest;
use the_beaconator::routes::wallet::fund_guest_wallet;

// Helper to create test app state
async fn create_test_state() -> the_beaconator::models::AppState {
    crate::test_utils::create_simple_test_app_state().await
}

/// Same fixture but override chain_id so we can assert against the mainnet guardrail.
async fn create_state_with_chain_id(chain_id: u64) -> the_beaconator::models::AppState {
    let mut state = crate::test_utils::create_simple_test_app_state().await;
    state.provider.chain_id = chain_id;
    state
}

#[tokio::test]
async fn test_fund_wallet_invalid_address() {
    let test_state = create_test_state().await;
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "invalid_address".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_empty_address() {
    let test_state = create_test_state().await;
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_invalid_usdc_amount() {
    let test_state = create_test_state().await;
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "not_a_number".to_string(),
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_invalid_eth_amount() {
    let test_state = create_test_state().await;
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "not_a_number".to_string(),
    });

    let result = fund_guest_wallet(state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_negative_usdc() {
    let test_state = create_test_state().await;
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "-1000000".to_string(),
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_negative_eth() {
    let test_state = create_test_state().await;
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "-1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_usdc_exceeds_limit() {
    let mut state = create_test_state().await;
    state.wallets.usdc_transfer_limit = 10_000_000; // 10 USDC
    let state = State::from(&state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "20000000".to_string(), // 20 USDC
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(state, request, token).await;
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
    assert!(response.into_inner().message.contains("exceeds limit"));
}

#[tokio::test]
async fn test_fund_wallet_eth_exceeds_limit() {
    let mut state = create_test_state().await;
    state.wallets.eth_transfer_limit = 1_000_000_000_000_000; // 0.001 ETH
    let state = State::from(&state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "2000000000000000".to_string(), // 0.002 ETH
    });

    let result = fund_guest_wallet(state, request, token).await;
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
    assert!(response.into_inner().message.contains("exceeds limit"));
}

#[tokio::test]
#[ignore = "requires WalletManager with Redis"]
async fn test_fund_wallet_zero_amounts() {
    let test_state = create_test_state().await;
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "0".to_string(),
        eth_amount: "0".to_string(),
    });

    // Zero amounts are technically valid, but will fail at network level
    let result = fund_guest_wallet(state, request, token).await;
    // Could be BadRequest or InternalServerError depending on validation
    assert!(result.is_err());
}

#[tokio::test]
#[ignore = "requires WalletManager with Redis"]
async fn test_fund_wallet_valid_format_network_failure() {
    let test_state = create_test_state().await;
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "1000000000000000".to_string(),
    });

    // Valid input but should fail due to network issues in test environment
    let result = fund_guest_wallet(state, request, token).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_fund_wallet_decimal_usdc_amount() {
    let test_state = create_test_state().await;
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "10.5".to_string(), // Decimals not allowed
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_scientific_notation() {
    let test_state = create_test_state().await;
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "1e6".to_string(), // Scientific notation
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
#[ignore = "requires WalletManager with Redis"]
async fn test_fund_wallet_address_with_mixed_case() {
    let test_state = create_test_state().await;
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    // Mixed case checksum address
    let request = Json(FundGuestWalletRequest {
        wallet_address: "0xAbCdEf1234567890123456789012345678901234".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "1000000000000000".to_string(),
    });

    // Should parse correctly but fail at network level
    let result = fund_guest_wallet(state, request, token).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_fund_wallet_max_u128_amounts() {
    let test_state = create_test_state().await;
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: u128::MAX.to_string(),
        eth_amount: u128::MAX.to_string(),
    });

    // Should fail due to exceeding limits
    let result = fund_guest_wallet(state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::BadRequest);
}

#[tokio::test]
async fn test_fund_wallet_disabled_on_arbitrum_one_mainnet() {
    // chain_id 42161 = Arbitrum One. fund_guest_wallet must refuse with 403 Forbidden
    // before any address/amount parsing happens.
    let test_state = create_state_with_chain_id(42161).await;
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(state, request, token).await;
    assert!(result.is_err());
    let (status, body) = result.unwrap_err();
    assert_eq!(status, Status::Forbidden);
    assert!(body.message.contains("disabled"));
    assert!(body.message.contains("42161"));
}

#[tokio::test]
async fn test_fund_wallet_disabled_on_unknown_production_chain() {
    // Anything not in the allow-list (421614 Arbitrum Sepolia / 31337 Anvil) is treated as
    // production. Use Base mainnet (8453) as the example: even if we accidentally re-enabled
    // Base, the funding endpoint stays disabled.
    let test_state = create_state_with_chain_id(8453).await;
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(state, request, token).await;
    assert!(result.is_err());
    let (status, _) = result.unwrap_err();
    assert_eq!(status, Status::Forbidden);
}

#[tokio::test]
#[ignore = "requires WalletManager with Redis"]
async fn test_fund_wallet_allowed_on_arbitrum_sepolia() {
    // chain_id 421614 = Arbitrum Sepolia. Guardrail must NOT fire — we should fall through
    // to the normal handler and (with the test stub provider) bottom out somewhere later.
    // Concretely we expect the request to NOT return Forbidden.
    let test_state = create_state_with_chain_id(421614).await;
    let state = State::from(&test_state);
    let token = ApiToken("test_token".to_string());

    let request = Json(FundGuestWalletRequest {
        wallet_address: "0x1234567890123456789012345678901234567890".to_string(),
        usdc_amount: "1000000".to_string(),
        eth_amount: "1000000000000000".to_string(),
    });

    let result = fund_guest_wallet(state, request, token).await;
    // Whether this succeeds or fails depends on the local provider; we only care that the
    // failure mode is *not* the mainnet guardrail.
    if let Err((status, body)) = result {
        assert_ne!(
            status,
            Status::Forbidden,
            "Arbitrum Sepolia (chain 421614) should NOT trip the mainnet guardrail, \
             but got Forbidden with message: {}",
            body.message
        );
    }
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

// --- top_up_pool + faucet ETH reserve floor ---

mod top_up_and_reserve {
    use super::*;
    use alloy::primitives::U256;
    use alloy::providers::Provider;
    use the_beaconator::guards::AdminToken;
    use the_beaconator::models::TopUpPoolRequest;
    use the_beaconator::routes::wallet::top_up_pool;

    fn admin() -> AdminToken {
        AdminToken("test_admin_token".to_string())
    }

    #[tokio::test]
    async fn test_top_up_pool_refused_on_production_chain() {
        let test_state = create_state_with_chain_id(42161).await;
        let state = State::from(&test_state);

        let request = Json(TopUpPoolRequest { usdc_target: None });
        let result = top_up_pool(state, request, admin()).await;

        assert!(result.is_err());
        let (status, response) = result.unwrap_err();
        assert_eq!(status, Status::Forbidden);
        assert!(response.message.contains("disabled on chain id 42161"));
    }

    #[tokio::test]
    async fn test_top_up_pool_rejects_invalid_target() {
        let test_state = create_test_state().await;
        let state = State::from(&test_state);

        for bad in ["not-a-number", "0", "-5"] {
            let request = Json(TopUpPoolRequest {
                usdc_target: Some(bad.to_string()),
            });
            let result = top_up_pool(State::from(&test_state), request, admin()).await;
            assert!(result.is_err(), "target {bad:?} must be rejected");
            let (status, _) = result.unwrap_err();
            assert_eq!(status, Status::BadRequest);
        }
        let _ = state;
    }

    #[tokio::test]
    async fn test_top_up_pool_empty_pool_unavailable() {
        // test_stub manager has no signers -> pool is empty -> 503.
        let test_state = create_test_state().await;
        let state = State::from(&test_state);

        let request = Json(TopUpPoolRequest { usdc_target: None });
        let result = top_up_pool(state, request, admin()).await;

        assert!(result.is_err());
        let (status, response) = result.unwrap_err();
        assert_eq!(status, Status::ServiceUnavailable);
        assert!(response.message.contains("pool is empty"));
    }

    /// Set an account's ETH balance on Anvil.
    async fn set_eth_balance(
        provider: &the_beaconator::ReadOnlyProvider,
        address: Address,
        wei: U256,
    ) {
        let _: () = provider
            .raw_request("anvil_setBalance".into(), (address, wei))
            .await
            .expect("anvil_setBalance");
    }

    #[tokio::test]
    #[ignore = "requires Redis + Anvil"]
    async fn test_fund_guest_wallet_refused_below_eth_reserve() {
        let (app_state, _anvil) =
            crate::test_utils::create_isolated_test_app_state_with_redis().await;

        // Drop every pool wallet to 0.015 ETH: enough for the 0.001 ETH
        // transfer alone, NOT enough once the 0.02 ETH reserve floor applies.
        for wallet in app_state.wallets.manager.signer_addresses() {
            set_eth_balance(
                &app_state.provider.read_provider,
                wallet,
                U256::from(15_000_000_000_000_000u128),
            )
            .await;
        }

        let state = State::from(&app_state);
        let request = Json(FundGuestWalletRequest {
            wallet_address: "0x742d35Cc6634C0532925a3b844Bc9e7595f8b94b".to_string(),
            usdc_amount: "1000000".to_string(),
            eth_amount: "1000000000000000".to_string(),
        });

        let result = fund_guest_wallet(state, request, ApiToken("test_token".to_string())).await;

        assert!(result.is_err(), "funding must be refused below the reserve");
        let (status, response) = result.unwrap_err();
        assert_eq!(status, Status::ServiceUnavailable);
        assert!(
            response.message.contains("reserve"),
            "unexpected message: {}",
            response.message
        );
    }

    #[tokio::test]
    #[ignore = "requires Redis + Anvil"]
    async fn test_top_up_pool_mints_wallets_to_target() {
        use crate::test_utils::{deploy_contract, load_contract_bytecode};
        use alloy::network::EthereumWallet;
        use alloy::providers::ProviderBuilder;
        use the_beaconator::routes::IERC20;

        let (mut app_state, anvil) =
            crate::test_utils::create_isolated_test_app_state_with_redis().await;

        // Deploy the permissionless-mint MockUSDC (same semantics as the
        // deployed testnet USDC) and point the app at it.
        let wallet = EthereumWallet::from(anvil.deployer_signer());
        let deploy_provider = std::sync::Arc::new(
            ProviderBuilder::new()
                .wallet(wallet)
                .connect_http(anvil.rpc_url().parse().expect("anvil url")),
        );
        let usdc = deploy_contract(&deploy_provider, load_contract_bytecode("MockUSDC"))
            .await
            .expect("deploy MockUSDC");
        app_state.contracts.usdc = usdc;

        let pool = app_state.wallets.manager.signer_addresses();
        assert!(!pool.is_empty());

        let state = State::from(&app_state);
        let request = Json(TopUpPoolRequest {
            usdc_target: Some("5000000".to_string()), // 5 USDC
        });

        let result = top_up_pool(state, request, admin()).await;
        let response = result.expect("top up should succeed").into_inner();
        assert!(response.success, "message: {}", response.message);

        let usdc_contract = IERC20::new(usdc, &*app_state.provider.read_provider);
        for wallet in pool {
            let balance = usdc_contract
                .balanceOf(wallet)
                .call()
                .await
                .expect("balanceOf");
            assert_eq!(balance, U256::from(5_000_000u64), "wallet {wallet}");
        }
    }
}
