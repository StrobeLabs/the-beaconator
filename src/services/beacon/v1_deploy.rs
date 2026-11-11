//! Create a v1 identity beacon (beacons@f762da1).
//!
//! The v1 deployment has no one-call `IdentityFactory`: only `ECDSAVerifierFactory` and
//! `IdentityBeaconFactory` are deployed (beacons `script/DeployFactories.s.sol` at f762da1).
//! `IdentityBeaconFactory.createBeacon` does not bind the verifier, so this flow does it:
//!
//! 1. `ECDSAVerifierFactory.createVerifier(signer)` from pool wallet W. W is the binder.
//! 2. `IdentityBeaconFactory.createBeacon(verifier, initialIndex)`.
//! 3. `verifier.bind(beacon)` from the same wallet W, then check `authorizedCaller()`.
//!
//! W is held for all three steps. Warm-up (the first update, nonce 1) and registration run
//! after W is released; see `routes::beacon::create_v1_identity_beacon`.

use alloy::primitives::{Address, B256, U256};
use std::time::Duration;
use tokio::time::timeout;

use super::ecdsa_deploy::create_ecdsa_verifier_at;
use super::v1::bind_to_beacon;
use crate::models::AppState;
use crate::routes::IIdentityBeaconFactory;
use crate::services::transaction::execution::send_with_nonce_retry;

pub struct V1IdentityBeacon {
    pub beacon: Address,
    pub verifier: Address,
    pub bind_tx: B256,
}

pub async fn create_v1_identity_beacon(
    state: &AppState,
    initial_index: U256,
) -> Result<V1IdentityBeacon, String> {
    let v1 = state
        .contracts
        .v1
        .as_ref()
        .ok_or("v1 is not configured: set the V1_* address env vars")?;

    let handle = state
        .wallets
        .manager
        .acquire_any_wallet()
        .await
        .map_err(|e| format!("Failed to acquire wallet: {e}"))?;
    let provider = handle
        .build_provider(&state.provider.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;
    tracing::info!(
        "Creating v1 identity beacon with wallet {} (binder)",
        handle.address()
    );

    let verifier = create_ecdsa_verifier_at(state, &handle, v1.ecdsa_verifier_factory).await?;

    let factory = IIdentityBeaconFactory::new(v1.identity_beacon_factory, &provider);
    let beacon = factory
        .createBeacon(verifier, initial_index)
        .call()
        .await
        .map_err(|e| format!("IdentityBeaconFactory.createBeacon preflight failed: {e}"))?;
    let pending = send_with_nonce_retry("v1 createBeacon", || {
        let call = factory.createBeacon(verifier, initial_index);
        let lock_ok = handle.ensure_lock_held();
        Box::pin(async move {
            lock_ok?;
            call.send()
                .await
                .map_err(|e| format!("Failed to send createBeacon: {e}"))
        })
    })
    .await?;
    let tx_hash = *pending.tx_hash();
    let receipt = timeout(Duration::from_secs(120), pending.get_receipt())
        .await
        .map_err(|_| format!("Timeout waiting for createBeacon receipt (tx {tx_hash})"))?
        .map_err(|e| format!("Failed to get createBeacon receipt (tx {tx_hash}): {e}"))?;
    if !receipt.status() {
        return Err(format!("createBeacon transaction {tx_hash} reverted"));
    }
    super::verify_deployed(&provider, beacon, "v1 IdentityBeacon").await?;

    let bind_tx = bind_to_beacon(&provider, verifier, beacon, "ECDSAVerifier", || {
        handle.ensure_lock_held()
    })
    .await?;

    tracing::info!("v1 identity beacon {beacon}: verifier {verifier} bound (tx {bind_tx})");
    Ok(V1IdentityBeacon {
        beacon,
        verifier,
        bind_tx,
    })
}
