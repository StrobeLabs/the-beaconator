use crate::test_utils::{AnvilManager, create_simple_test_app_state, load_contract_bytecode};
use alloy::{
    network::{EthereumWallet, TransactionBuilder},
    primitives::{Address, Bytes, U256},
    providers::{Provider, ProviderBuilder, ext::AnvilApi},
    rpc::types::TransactionRequest,
};
use rocket::{State, http::Status, serde::json::Json};
use the_beaconator::{
    guards::ApiToken,
    models::SafeConfig,
    routes::retirement::{RetirementIntent, RetirementRequest, retire_perp},
};

alloy::sol! {
    #[sol(rpc)]
    interface Harness {
        function configure(address module, int88 newRate, uint256 deadline) external;
        function setKnown(bool known) external;
        function setDisabled(bool value) external;
    }
}

fn request(perp: Address, intent: RetirementIntent) -> Json<RetirementRequest> {
    Json(RetirementRequest {
        perp_address: perp.to_string(),
        intent,
        propose: false,
        nonce: Some(7),
        expected_hash: None,
    })
}

#[tokio::test]
#[ignore = "requires Anvil and compiled Solidity fixtures"]
async fn retirement_enforces_chain_state_and_binds_exact_proposals() {
    let anvil = AnvilManager::new().await;
    let provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(anvil.deployer_signer()))
        .connect_http(anvil.rpc_url().parse().unwrap());
    let receipt = provider
        .send_transaction(
            TransactionRequest::default()
                .with_deploy_code(load_contract_bytecode("RetirementHarness")),
        )
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    let perp = receipt.contract_address.unwrap();
    let zero: Address = "0x8e562a533a92B47F9cF14300e42d6822a81aD4e1"
        .parse()
        .unwrap();
    provider
        .anvil_set_code(
            zero,
            Bytes::from(hex::decode("600060005260206000f3").unwrap()),
        )
        .await
        .unwrap();
    let mut app = create_simple_test_app_state().await;
    app.provider.read_provider = std::sync::Arc::new(
        the_beaconator::services::rpc::RpcConfig::build_read_only_provider(anvil.rpc_url())
            .unwrap(),
    );
    app.provider.chain_id = 42161;
    app.contracts.perp_factory = perp;
    app.contracts.perpcity_registry = perp;
    app.contracts.safe = Some(SafeConfig {
        address: perp,
        tx_service_url: Some("http://127.0.0.1:1".into()),
    });
    let harness = Harness::new(perp, &provider);
    harness
        .configure(
            app.contracts.funding_module,
            alloy::primitives::aliases::I88::ONE,
            U256::ZERO,
        )
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    let state = State::from(&app);
    let token = || ApiToken("test_token".into());
    let first = retire_perp(request(perp, RetirementIntent::Funding), token(), state)
        .await
        .unwrap()
        .into_inner()
        .data
        .unwrap();
    assert_eq!(first.step, "submit_funding");
    assert!(!first.funding_stopped);
    let repeated = retire_perp(request(perp, RetirementIntent::Funding), token(), state)
        .await
        .unwrap()
        .into_inner()
        .data
        .unwrap();
    assert_eq!(first.proposal_hash, repeated.proposal_hash);
    let mut wrong = request(perp, RetirementIntent::Funding);
    wrong.propose = true;
    wrong.expected_hash = Some(format!("0x{}", "00".repeat(32)));
    assert_eq!(
        retire_perp(wrong, token(), state).await.unwrap_err(),
        Status::Conflict
    );
    assert_eq!(
        retire_perp(request(perp, RetirementIntent::Beacon), token(), state)
            .await
            .unwrap_err(),
        Status::Conflict
    );
    harness
        .configure(zero, alloy::primitives::aliases::I88::ONE, U256::ZERO)
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    let before_touch = retire_perp(request(perp, RetirementIntent::Funding), token(), state)
        .await
        .unwrap()
        .into_inner()
        .data
        .unwrap();
    assert_eq!(before_touch.step, "refresh_rate");
    assert!(!before_touch.funding_stopped);
    harness
        .configure(zero, alloy::primitives::aliases::I88::ZERO, U256::ZERO)
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    let ready = retire_perp(request(perp, RetirementIntent::Funding), token(), state)
        .await
        .unwrap()
        .into_inner()
        .data
        .unwrap();
    assert!(ready.funding_stopped);
    assert!(ready.proposal_hash.is_none());
    assert_eq!(
        retire_perp(request(perp, RetirementIntent::Beacon), token(), state)
            .await
            .unwrap()
            .into_inner()
            .data
            .unwrap()
            .step,
        "unregister_beacon"
    );
    harness
        .setKnown(false)
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    assert_eq!(
        retire_perp(request(perp, RetirementIntent::Funding), token(), state)
            .await
            .unwrap_err(),
        Status::BadRequest
    );
}
