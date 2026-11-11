//! The v1 beacon kind (beacons@f762da1, deployed with perpcity-contracts v1.0.0).
//!
//! The old kind (beacons@v0.0.1) keeps working unchanged: old perps run until their outside
//! OI is 0. The update path selects the kind per beacon from the length of the `index()`
//! reply, never from a flag. What is different in v1:
//!
//! - `index()` returns `(index, updatedAt)`. The old kind returns `index` only.
//! - The ECDSA verifier accepts only nonce `lastNonce + 1`. The old kind accepts any unused
//!   proof, so the beaconator signs a timestamp nonce for it.
//! - The verifier (and a standalone base function) is caller-bound. Only its binder, the
//!   account that called the factory, can call `bind(beacon)`, and only once.
//! - A v1 perp reverts every action (close and liquidation too) when the beacon is older than
//!   the perp latency. So the beaconator publishes when the beacon is 900 s old or more, even
//!   when the value did not change and even when the publish rate limit would skip it.

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use alloy::primitives::{Address, Bytes, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use alloy::sol_types::SolCall;

use crate::routes::{IBeaconV1, ICallerBound, IEcdsaVerifierV1};

/// A v1 beacon this old (or older) gets a publish. Below this age a heartbeat request is
/// skipped. With the D1 latency floor of 3600 s this leaves room for two missed cycles.
pub const HEARTBEAT_MIN_AGE_SECS: u64 = 900;

/// Which verifier and nonce rules a beacon uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifierKind {
    /// beacons@v0.0.1: `index()` is one word; timestamp nonce.
    Legacy,
    /// beacons@f762da1: `index()` is `(index, updatedAt)`; nonce `lastNonce + 1`.
    V1,
}

/// One `index()` read. `updated_at` is `Some` only for v1 beacons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexReading {
    pub kind: VerifierKind,
    pub index: U256,
    pub updated_at: Option<U256>,
}

/// Decode the raw `index()` return data. The reply length is the kind.
pub fn decode_index_return(data: &[u8]) -> Result<IndexReading, String> {
    match data.len() {
        32 => Ok(IndexReading {
            kind: VerifierKind::Legacy,
            index: U256::from_be_slice(data),
            updated_at: None,
        }),
        64 => Ok(IndexReading {
            kind: VerifierKind::V1,
            index: U256::from_be_slice(&data[..32]),
            updated_at: Some(U256::from_be_slice(&data[32..])),
        }),
        n => Err(format!(
            "index() returned {n} bytes; expected 32 (old beacon) or 64 (v1 beacon)"
        )),
    }
}

/// Read `index()` with a raw `eth_call`, so one call serves both kinds.
pub async fn read_index<P: Provider>(
    provider: &P,
    beacon: Address,
) -> Result<IndexReading, String> {
    let tx = TransactionRequest::default()
        .to(beacon)
        .input(Bytes::from(IBeaconV1::indexCall {}.abi_encode()).into());
    let data = provider
        .call(tx)
        .await
        .map_err(|e| format!("index() read on beacon {beacon} failed: {e}"))?;
    decode_index_return(&data)
}

/// The kind of a beacon never changes, so remember it. The update path uses this only to
/// keep an old beacon publishing when an `index()` read fails, as it did before v1.
static KNOWN_KINDS: LazyLock<RwLock<HashMap<Address, VerifierKind>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub fn remember_kind(beacon: Address, kind: VerifierKind) {
    let mut map = KNOWN_KINDS.write().unwrap_or_else(|p| p.into_inner());
    map.insert(beacon, kind);
}

pub fn known_kind(beacon: Address) -> Option<VerifierKind> {
    let map = KNOWN_KINDS.read().unwrap_or_else(|p| p.into_inner());
    map.get(&beacon).copied()
}

/// Seconds since `updated_at`. A future timestamp counts as age 0.
pub fn age_secs(updated_at: U256, now_secs: u64) -> u64 {
    let updated = u64::try_from(updated_at).unwrap_or(u64::MAX);
    now_secs.saturating_sub(updated)
}

pub fn unix_now_secs() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|e| format!("Failed to get system time: {e}"))
}

/// What the update path does with a v1 beacon of age `age_secs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V1Gate {
    /// A heartbeat request on a beacon younger than [`HEARTBEAT_MIN_AGE_SECS`]: send nothing.
    SkipHeartbeat { age_secs: u64 },
    /// The beacon is [`HEARTBEAT_MIN_AGE_SECS`] old or more: publish now. The rate limit and
    /// the value dedupe do not apply, because a stale v1 beacon stops the market.
    PublishNow { age_secs: u64 },
    /// A normal update on a fresh beacon: the normal rate limit and value dedupe apply.
    Normal { age_secs: u64 },
}

pub fn gate(heartbeat: bool, age_secs: u64) -> V1Gate {
    if age_secs >= HEARTBEAT_MIN_AGE_SECS {
        V1Gate::PublishNow { age_secs }
    } else if heartbeat {
        V1Gate::SkipHeartbeat { age_secs }
    } else {
        V1Gate::Normal { age_secs }
    }
}

/// The v1 verifier requires `nonce == lastNonce + 1`.
pub fn next_nonce(last_nonce: U256) -> Result<U256, String> {
    last_nonce
        .checked_add(U256::from(1))
        .ok_or_else(|| "verifier lastNonce is at uint256 max".to_string())
}

/// After a receipt did not arrive: the update with `sent_nonce` is on chain exactly when
/// the verifier has accepted that nonce. The per-beacon update lock makes our tx the only
/// one that can use `sent_nonce`.
pub fn landed(last_nonce_now: U256, sent_nonce: U256) -> bool {
    last_nonce_now >= sent_nonce
}

pub async fn read_last_nonce<P: Provider>(provider: &P, verifier: Address) -> Result<U256, String> {
    IEcdsaVerifierV1::new(verifier, provider)
        .lastNonce()
        .call()
        .await
        .map_err(|e| format!("lastNonce() read on verifier {verifier} failed: {e}"))
}

/// Highest nonce this process saw confirmed, per verifier. A lagging RPC replica can return
/// an old `lastNonce` right after a confirmed update; signing `old + 1` again would revert
/// on chain. The floor only moves up, and only on a confirmed receipt or a nonce re-read.
static CONFIRMED_NONCES: LazyLock<RwLock<HashMap<Address, U256>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub fn record_confirmed_nonce(verifier: Address, nonce: U256) {
    let mut map = CONFIRMED_NONCES.write().unwrap_or_else(|p| p.into_inner());
    let entry = map.entry(verifier).or_insert(nonce);
    if nonce > *entry {
        *entry = nonce;
    }
}

/// The nonce to sign for the next v1 update: one more than the higher of the chain read and
/// the local confirmed floor.
pub fn nonce_for_update(verifier: Address, chain_last_nonce: U256) -> Result<U256, String> {
    let floor = {
        let map = CONFIRMED_NONCES.read().unwrap_or_else(|p| p.into_inner());
        map.get(&verifier).copied().unwrap_or_default()
    };
    next_nonce(chain_last_nonce.max(floor))
}

/// Send `target.bind(beacon)` and check that `authorizedCaller()` is now `beacon`. The
/// sender must be the binder (the account that created `target` through its factory).
/// `ensure_lock` runs before each send attempt (pass the wallet lock check).
pub async fn bind_to_beacon<P, F>(
    provider: &P,
    target: Address,
    beacon: Address,
    label: &str,
    ensure_lock: F,
) -> Result<alloy::primitives::B256, String>
where
    P: Provider,
    F: Fn() -> Result<(), String> + Send + Sync,
{
    let contract = ICallerBound::new(target, provider);

    // Preflight: a wrong binder or a second bind reverts here, before any gas is spent.
    contract
        .bind(beacon)
        .call()
        .await
        .map_err(|e| format!("{label}.bind({beacon}) preflight reverted: {e}"))?;

    let pending = crate::services::transaction::execution::send_with_nonce_retry(
        &format!("{label}.bind"),
        || {
            let call = contract.bind(beacon);
            let lock_ok = ensure_lock();
            Box::pin(async move {
                lock_ok?;
                call.send()
                    .await
                    .map_err(|e| format!("Failed to send {label}.bind: {e}"))
            })
        },
    )
    .await?;
    let tx_hash = *pending.tx_hash();
    let receipt = tokio::time::timeout(std::time::Duration::from_secs(120), pending.get_receipt())
        .await
        .map_err(|_| format!("Timeout waiting for {label}.bind receipt (tx {tx_hash})"))?
        .map_err(|e| format!("Failed to get {label}.bind receipt (tx {tx_hash}): {e}"))?;
    if !receipt.status() {
        return Err(format!("{label}.bind transaction {tx_hash} reverted"));
    }

    let bound = contract
        .authorizedCaller()
        .call()
        .await
        .map_err(|e| format!("{label}.authorizedCaller() read failed: {e}"))?;
    if bound != beacon {
        return Err(format!(
            "{label} at {target} is bound to {bound}, expected beacon {beacon} (tx {tx_hash})"
        ));
    }
    Ok(tx_hash)
}
