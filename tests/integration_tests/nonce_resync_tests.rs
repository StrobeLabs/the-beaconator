//! Nonce resync-and-retry regression tests (Anvil-backed, self-skipping).
//!
//! The 2026-08-24 prod failure: every request path builds a fresh provider
//! whose nonce filler seeds itself from `get_transaction_count`, and behind a
//! load-balanced RPC that read can lag right after a prior tx from the same
//! wallet lands — the send then dies with "nonce too low". The remedy is
//! `is_nonce_error` -> `resynced_nonce` (pending count) -> one explicit-nonce
//! retry. A single local node cannot reproduce the lagging read itself, so
//! this test drives the same failure by re-sending a spent nonce and proves
//! every moving part of the remedy against the real node error.

use serial_test::serial;

/// End-to-end mechanics: a spent nonce produces an error that
/// `is_nonce_error` classifies, `resynced_nonce` reads the corrected pending
/// count, and an explicit-nonce retry lands.
#[tokio::test]
#[serial]
async fn test_nonce_resync_retry_mechanics_against_anvil() {
    use alloy::network::TransactionBuilder;
    use alloy::node_bindings::Anvil;
    use alloy::primitives::U256;
    use alloy::providers::{Provider, ProviderBuilder};
    use alloy::rpc::types::TransactionRequest;
    use alloy::signers::local::PrivateKeySigner;
    use the_beaconator::services::transaction::{is_nonce_error, resynced_nonce};

    let anvil = match Anvil::new().try_spawn() {
        Ok(a) => a,
        Err(e) => {
            println!("anvil not available ({e}); skipping");
            return;
        }
    };
    let signer: PrivateKeySigner = anvil.keys()[0].clone().into();
    let sender = signer.address();
    let provider = ProviderBuilder::new()
        .wallet(alloy::network::EthereumWallet::from(signer))
        .connect_http(anvil.endpoint_url());

    let start_nonce = provider
        .get_transaction_count(sender)
        .pending()
        .await
        .expect("read start nonce");

    // Land a tx at the current nonce.
    let tx = TransactionRequest::default()
        .with_to(sender)
        .with_value(U256::ZERO)
        .with_nonce(start_nonce);
    provider
        .send_transaction(tx)
        .await
        .expect("first send")
        .get_receipt()
        .await
        .expect("first receipt");

    // Re-sending the spent nonce is the same failure the lagging fill
    // produces; the node's real error wording must be classified.
    let stale = TransactionRequest::default()
        .with_to(sender)
        .with_value(U256::ZERO)
        .with_nonce(start_nonce);
    let err = provider
        .send_transaction(stale)
        .await
        .expect_err("spent nonce must be rejected");
    let msg = format!("{err}");
    assert!(
        is_nonce_error(&msg),
        "node nonce error not classified by is_nonce_error: {msg}"
    );

    // The remedy: resync from pending count and retry with it explicitly.
    let resynced = resynced_nonce(&provider, sender, "test")
        .await
        .expect("resync");
    assert_eq!(
        resynced,
        start_nonce + 1,
        "pending count reflects the landed tx"
    );

    let retry = TransactionRequest::default()
        .with_to(sender)
        .with_value(U256::ZERO)
        .with_nonce(resynced);
    let receipt = provider
        .send_transaction(retry)
        .await
        .expect("retry send")
        .get_receipt()
        .await
        .expect("retry receipt");
    assert!(receipt.status(), "retry with resynced nonce must land");
}
