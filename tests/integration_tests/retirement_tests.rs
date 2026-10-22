use crate::test_utils::{AnvilManager, create_simple_test_app_state, load_contract_bytecode};
use alloy::{
    network::{EthereumWallet, TransactionBuilder},
    primitives::{Address, Bytes, U256, aliases::I88},
    providers::{Provider, ProviderBuilder, ext::AnvilApi},
    rpc::types::TransactionRequest,
    sol_types::SolCall,
};
use rocket::{State, http::Status, serde::json::Json};
use the_beaconator::{
    guards::ApiToken,
    models::SafeConfig,
    routes::retirement::{
        MULTI_SEND_CALL_ONLY, RetirementIntent, RetirementRequest, multi_send_data, retire_perp,
    },
    services::safe::{SafeCall, SafeOperation, SafeTransactionService},
};

alloy::sol! {
    #[sol(rpc)]
    interface Harness {
        function configure(address module, int88 newRate, uint256 deadline) external;
        function setKnown(bool known) external;
        function setDisabled(bool value) external;
        function setOwner(address value) external;
        function setTimelock(uint256 value) external;
        function setTouchFails(bool value) external;
        function funding() external view returns (address);
        function rate() external view returns (int88);
        function registered() external view returns (bool);
        function setFundingModule(address module) external;
        function touch() external;
        function unregisterBeacon(address beacon) external;
    }
}

fn request(perp: Address, intent: RetirementIntent) -> Json<RetirementRequest> {
    Json(RetirementRequest {
        perp_address: perp.to_string(),
        intent,
        propose: false,
        nonce: Some(7),
        expected_hash: None,
        unregister_beacon: false,
        status_only: false,
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
        api_key: None,
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
    // Allocating a nonce needs the Safe service, which is unreachable here...
    let mut fresh = request(perp, RetirementIntent::Funding);
    fresh.nonce = None;
    assert_eq!(
        retire_perp(fresh, token(), state).await.unwrap_err(),
        Status::BadGateway
    );
    // ...but a status check never asks it, so polling spends no Safe quota.
    let mut observe = request(perp, RetirementIntent::Funding);
    observe.nonce = None;
    observe.status_only = true;
    let observed = retire_perp(observe, token(), state)
        .await
        .unwrap()
        .into_inner()
        .data
        .unwrap();
    assert_eq!(observed.step, "submit_funding");
    assert_eq!(observed.safe_nonce, first.safe_nonce);
    assert!(observed.nonce.is_none() && observed.proposal_hash.is_none());
    let mut contradictory = request(perp, RetirementIntent::Funding);
    contradictory.status_only = true;
    contradictory.propose = true;
    contradictory.expected_hash = first.proposal_hash.clone();
    assert_eq!(
        retire_perp(contradictory, token(), state)
            .await
            .unwrap_err(),
        Status::BadRequest
    );
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

/// Deploy a compiled test fixture and return its address.
async fn deploy<P: Provider>(provider: &P, name: &str) -> Address {
    provider
        .send_transaction(
            TransactionRequest::default().with_deploy_code(load_contract_bytecode(name)),
        )
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap()
        .contract_address
        .unwrap()
}

/// With no timelock, funding shutdown is one Safe batch, simulated as the Safe.
#[tokio::test]
#[ignore = "requires Anvil and compiled Solidity fixtures"]
async fn zero_timelock_funding_shutdown_is_one_simulated_safe_batch() {
    let anvil = AnvilManager::new().await;
    let provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(anvil.deployer_signer()))
        .connect_http(anvil.rpc_url().parse().unwrap());
    let perp = deploy(&provider, "RetirementHarness").await;
    let safe = deploy(&provider, "RetirementHarness").await;
    let multi_send = deploy(&provider, "MultiSendCallOnly").await;
    provider
        .anvil_set_code(
            MULTI_SEND_CALL_ONLY,
            provider.get_code_at(multi_send).await.unwrap(),
        )
        .await
        .unwrap();
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
        address: safe,
        tx_service_url: Some("http://127.0.0.1:1".into()),
        api_key: None,
    });
    let harness = Harness::new(perp, &provider);
    harness
        .setOwner(safe)
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
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
    let propose = |hash: Option<String>| {
        let mut request = request(perp, RetirementIntent::Funding);
        request.propose = true;
        request.expected_hash = hash;
        request
    };

    let batch = retire_perp(request(perp, RetirementIntent::Funding), token(), state)
        .await
        .unwrap()
        .into_inner()
        .data
        .unwrap();
    assert_eq!(batch.step, "submit_funding");
    assert!(batch.batched);
    // The simulation passes, so the proposal reaches the (unreachable) Safe service.
    assert_eq!(
        retire_perp(propose(batch.proposal_hash.clone()), token(), state)
            .await
            .unwrap_err(),
        Status::BadGateway
    );

    harness
        .setTouchFails(true)
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    assert_eq!(
        retire_perp(propose(batch.proposal_hash.clone()), token(), state)
            .await
            .unwrap_err(),
        Status::Conflict
    );

    harness
        .setTimelock(U256::from(60))
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    let stepwise = retire_perp(request(perp, RetirementIntent::Funding), token(), state)
        .await
        .unwrap()
        .into_inner()
        .data
        .unwrap();
    assert!(!stepwise.batched);
    assert_ne!(stepwise.proposal_hash, batch.proposal_hash);
}

/// Retirement is one Safe transaction: apply the zero module, refresh the rate and remove
/// the beacon, executed as the Safe.
#[tokio::test]
#[ignore = "requires Anvil and compiled Solidity fixtures"]
async fn retire_is_one_safe_batch_with_beacon_removal() {
    let anvil = AnvilManager::new().await;
    let provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(anvil.deployer_signer()))
        .connect_http(anvil.rpc_url().parse().unwrap());
    let perp = deploy(&provider, "RetirementHarness").await;
    let safe = deploy(&provider, "RetirementHarness").await;
    let multi_send = deploy(&provider, "MultiSendCallOnly").await;
    let multi_send_code = provider.get_code_at(multi_send).await.unwrap();
    provider
        .anvil_set_code(MULTI_SEND_CALL_ONLY, multi_send_code.clone())
        .await
        .unwrap();
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
        address: safe,
        tx_service_url: Some("http://127.0.0.1:1".into()),
        api_key: None,
    });
    let harness = Harness::new(perp, &provider);
    harness
        .setOwner(safe)
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    // Submitted earlier and executable now: the setter is the next funding step.
    harness
        .configure(app.contracts.funding_module, I88::ONE, U256::from(1))
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    let state = State::from(&app);
    let token = || ApiToken("test_token".into());
    let retire = |unregister_beacon: bool| {
        let mut request = request(perp, RetirementIntent::Retire);
        request.unregister_beacon = unregister_beacon;
        request
    };

    let plan = retire_perp(retire(true), token(), state)
        .await
        .unwrap()
        .into_inner()
        .data
        .unwrap();
    assert_eq!(plan.step, "set_funding");
    assert!(plan.batched);
    assert!(plan.includes_beacon);
    let calls = vec![
        (
            perp,
            Harness::setFundingModuleCall { module: zero }.abi_encode(),
        ),
        (perp, Harness::touchCall {}.abi_encode()),
        (
            perp,
            Harness::unregisterBeaconCall { beacon: perp }.abi_encode(),
        ),
    ];
    let data = multi_send_data(&calls);
    let hash = SafeTransactionService::encode_safe_tx_hash(
        safe,
        42161,
        SafeCall {
            to: MULTI_SEND_CALL_ONLY,
            data: &data,
            operation: SafeOperation::DelegateCall,
        },
        7,
    );
    assert_eq!(plan.proposal_hash, Some(format!("{hash:#x}")));

    // A timelock only delays the setter; once it is executable the beacon still joins the batch.
    harness
        .setTimelock(U256::from(60))
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    let timelocked = retire_perp(retire(true), token(), state)
        .await
        .unwrap()
        .into_inner()
        .data
        .unwrap();
    assert_eq!(timelocked.step, "set_funding");
    assert!(timelocked.includes_beacon);
    assert_eq!(timelocked.proposal_hash, plan.proposal_hash);
    harness
        .setTimelock(U256::ZERO)
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();

    let funding_only = retire_perp(retire(false), token(), state)
        .await
        .unwrap()
        .into_inner()
        .data
        .unwrap();
    assert!(!funding_only.includes_beacon);
    assert_ne!(funding_only.proposal_hash, plan.proposal_hash);

    // The simulation passes, so the proposal reaches the (unreachable) Safe service.
    let mut propose = retire(true);
    propose.propose = true;
    propose.expected_hash = plan.proposal_hash.clone();
    assert_eq!(
        retire_perp(propose, token(), state).await.unwrap_err(),
        Status::BadGateway
    );

    // Execute the batch as the Safe would: MultiSend's code running at the Safe address.
    let safe_code = provider.get_code_at(safe).await.unwrap();
    provider
        .anvil_set_code(safe, multi_send_code)
        .await
        .unwrap();
    provider
        .send_transaction(
            TransactionRequest::default()
                .to(safe)
                .input(Bytes::from(data).into()),
        )
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    provider.anvil_set_code(safe, safe_code).await.unwrap();
    assert_eq!(harness.funding().call().await.unwrap(), zero);
    assert_eq!(harness.rate().call().await.unwrap(), I88::ZERO);
    assert!(!harness.registered().call().await.unwrap());

    let done = retire_perp(retire(true), token(), state)
        .await
        .unwrap()
        .into_inner()
        .data
        .unwrap();
    assert_eq!(done.step, "retired");
    assert!(done.funding_stopped);
    assert!(!done.beacon_registered);
    assert!(done.proposal_hash.is_none());
}

/// Once funding is stopped, retirement proposes beacon removal alone.
#[tokio::test]
#[ignore = "requires Anvil and compiled Solidity fixtures"]
async fn retire_after_funding_stop_proposes_only_beacon_removal() {
    let anvil = AnvilManager::new().await;
    let provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(anvil.deployer_signer()))
        .connect_http(anvil.rpc_url().parse().unwrap());
    let perp = deploy(&provider, "RetirementHarness").await;
    let safe = deploy(&provider, "RetirementHarness").await;
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
        address: safe,
        tx_service_url: Some("http://127.0.0.1:1".into()),
        api_key: None,
    });
    let harness = Harness::new(perp, &provider);
    harness
        .setOwner(safe)
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    harness
        .configure(zero, I88::ZERO, U256::ZERO)
        .send()
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();
    let state = State::from(&app);
    let token = || ApiToken("test_token".into());
    let mut retire = request(perp, RetirementIntent::Retire);
    retire.unregister_beacon = true;
    let plan = retire_perp(retire, token(), state)
        .await
        .unwrap()
        .into_inner()
        .data
        .unwrap();
    assert_eq!(plan.step, "unregister_beacon");
    assert!(plan.includes_beacon);
    assert!(!plan.batched);
    let beacon = retire_perp(request(perp, RetirementIntent::Beacon), token(), state)
        .await
        .unwrap()
        .into_inner()
        .data
        .unwrap();
    assert_eq!(beacon.proposal_hash, plan.proposal_hash);

    let funding = retire_perp(request(perp, RetirementIntent::Retire), token(), state)
        .await
        .unwrap()
        .into_inner()
        .data
        .unwrap();
    assert_eq!(funding.step, "retired");
    assert!(funding.proposal_hash.is_none());
}
