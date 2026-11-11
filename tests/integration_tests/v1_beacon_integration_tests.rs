//! Level 1 on-chain tests for the v1 beacon kind (plan row 2c), on Anvil with mock contracts
//! that copy the beacons@f762da1 rules (binder-only `bind`, nonce == lastNonce + 1,
//! `index()` returns `(index, updatedAt)`). Direct signer provider, no Redis.

use alloy::network::{EthereumWallet, TransactionBuilder};
use alloy::primitives::{Address, Bytes, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::signers::Signer;
use alloy::signers::local::PrivateKeySigner;
use alloy::sol_types::{SolType, SolValue};
use serial_test::serial;
use the_beaconator::routes::{IBeaconV1, ICallerBound, IEcdsaVerifierV1};
use the_beaconator::services::beacon::v1::{self, VerifierKind};

use crate::test_utils::{AnvilManager, load_contract_bytecode};

async fn deploy(provider: &impl Provider, name: &str, args: Vec<u8>) -> Address {
    let mut code = load_contract_bytecode(name);
    code.extend_from_slice(&args);
    let tx = TransactionRequest::default().with_deploy_code(code);
    provider
        .send_transaction(tx)
        .await
        .expect("deploy send")
        .get_receipt()
        .await
        .expect("deploy receipt")
        .contract_address
        .expect("deployed contract address")
}

/// Sign `measurement` with `nonce` the way the beaconator does: digest from the verifier,
/// signature r || s || v, inputs abi.encode(uint256[], uint256).
async fn signed_update(
    provider: &impl Provider,
    signer: &PrivateKeySigner,
    verifier: Address,
    measurement: U256,
    nonce: U256,
) -> (Bytes, Bytes) {
    let digest = IEcdsaVerifierV1::new(verifier, provider)
        .digest(vec![measurement], nonce)
        .call()
        .await
        .expect("digest()");
    let signature = signer.sign_hash(&digest).await.expect("sign");
    let inputs = <(
        alloy::sol_types::sol_data::Array<alloy::sol_types::sol_data::Uint<256>>,
        alloy::sol_types::sol_data::Uint<256>,
    )>::abi_encode_params(&(vec![measurement], nonce));
    (
        Bytes::from(signature.as_bytes().to_vec()),
        Bytes::from(inputs),
    )
}

#[tokio::test]
#[serial]
async fn v1_bind_nonce_sequence_and_unconfirmed_reread() {
    let anvil = AnvilManager::new().await;
    let deployer = anvil.deployer_signer();
    let binder = deployer.address();
    let provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(deployer))
        .connect_http(anvil.rpc_url().parse().expect("rpc url"));
    let measurement_signer = anvil.get_signer(1);

    let initial = U256::from(1u8) << 96;
    let verifier = deploy(
        &provider,
        "MockV1Verifier",
        (measurement_signer.address(), binder).abi_encode_params(),
    )
    .await;
    let beacon = deploy(
        &provider,
        "MockV1Beacon",
        (verifier, initial).abi_encode_params(),
    )
    .await;

    // Kind: a v1 beacon answers index() with two words.
    let reading = v1::read_index(&provider, beacon).await.expect("index()");
    assert_eq!(reading.kind, VerifierKind::V1);
    assert_eq!(reading.index, initial);
    assert!(reading.updated_at.is_some());

    // bind: a wallet that is not the binder is refused before any send.
    let other = ProviderBuilder::new()
        .wallet(EthereumWallet::from(anvil.get_signer(2)))
        .connect_http(anvil.rpc_url().parse().expect("rpc url"));
    assert!(
        v1::bind_to_beacon(&other, verifier, beacon, "verifier", || Ok(()))
            .await
            .is_err(),
        "only the binder may bind"
    );
    // The binder binds, and the helper checks authorizedCaller().
    v1::bind_to_beacon(&provider, verifier, beacon, "verifier", || Ok(()))
        .await
        .expect("binder binds the verifier");
    let bound = ICallerBound::new(verifier, &provider)
        .authorizedCaller()
        .call()
        .await
        .expect("authorizedCaller()");
    assert_eq!(bound, beacon);
    // A second bind is refused (AlreadyBound).
    assert!(
        v1::bind_to_beacon(&provider, verifier, beacon, "verifier", || Ok(()))
            .await
            .is_err(),
        "bind is one-time"
    );
    // A lost wallet lock stops the send.
    let fresh_verifier = deploy(
        &provider,
        "MockV1Verifier",
        (measurement_signer.address(), binder).abi_encode_params(),
    )
    .await;
    assert!(
        v1::bind_to_beacon(&provider, fresh_verifier, beacon, "verifier", || {
            Err("lock lost".to_string())
        })
        .await
        .is_err()
    );

    // Nonce sequence: 1, 2, 3, each from lastNonce + 1.
    let beacon_contract = IBeaconV1::new(beacon, &provider);
    for expected in 1u64..=3 {
        let last = v1::read_last_nonce(&provider, verifier)
            .await
            .expect("lastNonce()");
        let nonce = v1::next_nonce(last).expect("next nonce");
        assert_eq!(nonce, U256::from(expected));

        // Unconfirmed-receipt rule: before the tx lands, lastNonce < nonce.
        assert!(!v1::landed(last, nonce));

        let value = initial + U256::from(expected);
        let (proof, inputs) =
            signed_update(&provider, &measurement_signer, verifier, value, nonce).await;
        let receipt = beacon_contract
            .update(proof, inputs)
            .send()
            .await
            .expect("update send")
            .get_receipt()
            .await
            .expect("update receipt");
        assert!(receipt.status());

        // The re-read after the tx lands sees our nonce: it counts as confirmed.
        let after = v1::read_last_nonce(&provider, verifier)
            .await
            .expect("lastNonce()");
        assert_eq!(after, nonce);
        assert!(v1::landed(after, nonce));
        let reading = v1::read_index(&provider, beacon).await.expect("index()");
        assert_eq!(reading.index, value);
    }

    // A reused nonce and a skipped nonce both revert.
    for bad in [U256::from(3), U256::from(5)] {
        let (proof, inputs) =
            signed_update(&provider, &measurement_signer, verifier, initial, bad).await;
        assert!(
            beacon_contract.update(proof, inputs).call().await.is_err(),
            "nonce {bad} must revert when lastNonce is 3"
        );
    }

    // An old-kind beacon answers index() with one word.
    let legacy = deploy(&provider, "MockLegacyIndexBeacon", Vec::new()).await;
    let reading = v1::read_index(&provider, legacy).await.expect("index()");
    assert_eq!(reading.kind, VerifierKind::Legacy);
    assert_eq!(reading.updated_at, None);
}
