//! On-chain integration test for the touch-via-Multicall3 mechanic.
//!
//! Proves that `touch_calls` + `Multicall3.aggregate3(allowFailure = true)`:
//!   1. executes `touch()` on a healthy perp (its counter advances), and
//!   2. isolates a reverting perp - a bad sub-call does NOT revert the batch.
//!
//! This is the worker's on-chain send path (a direct mirror of batch.rs), run
//! against a local Anvil with mock contracts. It uses a direct signer provider,
//! not the Redis-backed wallet pool, so it needs only Anvil (no Redis).

use alloy::network::{EthereumWallet, TransactionBuilder};
use alloy::primitives::{Address, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use serial_test::serial;
use the_beaconator::routes::IMulticall3;
use the_beaconator::services::touch::touch_calls;

use crate::test_utils::{AnvilManager, load_contract_bytecode};

alloy::sol! {
    #[sol(rpc)]
    interface IMockTouchable {
        function touchCount() external view returns (uint256);
    }
}

async fn deploy(provider: &impl Provider, name: &str) -> Address {
    let bytecode = load_contract_bytecode(name);
    let tx = TransactionRequest::default().with_deploy_code(bytecode);
    let receipt = provider
        .send_transaction(tx)
        .await
        .expect("deploy send")
        .get_receipt()
        .await
        .expect("deploy receipt");
    receipt.contract_address.expect("deployed contract address")
}

#[tokio::test]
#[serial]
#[ignore = "requires Anvil (run in the integration CI job)"]
async fn touch_via_multicall_touches_healthy_perp_and_isolates_reverting_one() {
    let anvil = AnvilManager::new().await;
    let wallet = EthereumWallet::from(anvil.deployer_signer());
    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(anvil.rpc_url().parse().expect("rpc url"));

    // Multicall3 + a healthy perp + a perp whose touch() always reverts.
    let multicall_addr = deploy(&provider, "MockMulticall3").await;
    let good = deploy(&provider, "MockTouchable").await;
    let bad = deploy(&provider, "RevertingTouchable").await;

    // The worker touches every perp for a beacon in one batch. Put the reverting
    // perp in the middle to prove the good ones on either side still execute.
    let good2 = deploy(&provider, "MockTouchable").await;
    let calls = touch_calls(&[good, bad, good2]);
    assert_eq!(calls.len(), 3);

    let multicall = IMulticall3::new(multicall_addr, &provider);

    // Simulate: allowFailure means the batch does not revert; the middle call
    // reports success = false, the healthy ones report success = true.
    let results = multicall
        .aggregate3(calls.clone())
        .call()
        .await
        .expect("aggregate3 simulation must not revert with allowFailure");
    assert!(results[0].success, "healthy perp touch should succeed");
    assert!(
        !results[1].success,
        "reverting perp touch should fail (isolated)"
    );
    assert!(
        results[2].success,
        "second healthy perp touch should succeed"
    );

    // Execute for real and confirm the batch tx itself succeeded.
    let receipt = multicall
        .aggregate3(calls)
        .send()
        .await
        .expect("aggregate3 send")
        .get_receipt()
        .await
        .expect("aggregate3 receipt");
    assert!(receipt.status(), "touch batch tx must not revert");

    // Both healthy perps were actually touched on-chain; the reverting one made
    // no state change but did not block the others.
    for perp in [good, good2] {
        let count = IMockTouchable::new(perp, &provider)
            .touchCount()
            .call()
            .await
            .expect("touchCount");
        assert_eq!(count, U256::from(1), "healthy perp should be touched once");
    }
}
