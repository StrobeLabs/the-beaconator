use alloy::primitives::{Address, B256, Bytes, U256};
use alloy::providers::Provider;
use alloy::signers::Signer;
use alloy::sol_types::SolType;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::timeout;
use tracing;

use super::v1::{self, IndexReading, V1Gate, VerifierKind};
use crate::ReadOnlyProvider;
use crate::models::{AppState, UpdateBeaconWithEcdsaRequest};
use crate::routes::{IBeacon, IEcdsaVerifier};
use crate::services::touch::Tier;
use crate::services::transaction::execution::{is_insufficient_funds_error, send_with_nonce_retry};
use crate::services::wallet::{LockHeartbeat, WalletHandle, WalletLockGuard};

/// How long a sent-but-unresolved update tx keeps its beacon lock alive while a
/// background watcher polls for the receipt, and how often it polls.
const PENDING_TX_LOCK_GRACE: Duration = Duration::from_secs(240);
const PENDING_TX_POLL_INTERVAL: Duration = Duration::from_secs(3);
/// Per-poll RPC timeout so one hung request cannot stall the watcher loop
/// (deadline is only checked between polls).
const PENDING_TX_POLL_TIMEOUT: Duration = Duration::from_secs(10);

/// Keep the per-beacon update lock held while a SENT update tx is still
/// unresolved, so a second update cannot race it on the verifier nonce. The
/// lock is released (guard drop) as soon as the tx gets a receipt, or when the
/// grace window ends; if this instance dies, the lock's Redis TTL expires it.
///
/// `v1_nonce` is `(verifier, nonce)` for a v1 update: a late successful
/// receipt records it as confirmed, so the next update does not depend on a
/// replica that may still return the old `lastNonce`.
fn hold_beacon_lock_until_receipt(
    lock: (LockHeartbeat, WalletLockGuard),
    provider: Arc<ReadOnlyProvider>,
    tx_hash: B256,
    beacon_address: Address,
    v1_nonce: Option<(Address, U256)>,
) {
    tokio::spawn(async move {
        // Tuple order matches the manager's return: heartbeat drops before the
        // guard releases when this task ends.
        let _lock = lock;
        let deadline = tokio::time::Instant::now() + PENDING_TX_LOCK_GRACE;
        while tokio::time::Instant::now() < deadline {
            match timeout(
                PENDING_TX_POLL_TIMEOUT,
                provider.get_transaction_receipt(tx_hash),
            )
            .await
            {
                Ok(Ok(Some(receipt))) => {
                    if let Some((verifier, nonce)) = v1_nonce
                        && receipt.status()
                    {
                        v1::record_confirmed_nonce(verifier, nonce);
                    }
                    tracing::info!(
                        "Pending update tx {tx_hash} for beacon {beacon_address} resolved; releasing beacon update lock"
                    );
                    return;
                }
                Ok(Ok(None)) => {}
                Ok(Err(e)) => {
                    tracing::warn!("Receipt poll for pending update tx {tx_hash} failed: {e}");
                }
                Err(_) => {
                    tracing::warn!(
                        "Receipt poll for pending update tx {tx_hash} timed out after {PENDING_TX_POLL_TIMEOUT:?}"
                    );
                }
            }
            tokio::time::sleep(PENDING_TX_POLL_INTERVAL).await;
        }
        tracing::warn!(
            "Releasing beacon {beacon_address} update lock: tx {tx_hash} still unresolved after {PENDING_TX_LOCK_GRACE:?} grace window"
        );
    });
}

/// Whether a publish feeds the rate limit and the value dedupe. A v1
/// heartbeat does not: it only refreshes `updatedAt`, and recording it would
/// let the rate limit skip a changed value that arrives just after it, so the
/// old value would stay on chain. The v1 age gate reads `updatedAt` on chain,
/// not this record.
fn records_publish(kind: VerifierKind, heartbeat: bool) -> bool {
    !(heartbeat && kind == VerifierKind::V1)
}

/// Why an update produced no transaction. Logging detail only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The beacon published too recently (see
    /// [`super::dedupe::min_publish_interval`]).
    RateLimited { elapsed: u64, interval: u64 },
    /// The on-chain value already equals the measurement and the heartbeat has
    /// not elapsed.
    ValueUnchanged,
    /// A heartbeat request on a v1 beacon younger than
    /// [`v1::HEARTBEAT_MIN_AGE_SECS`].
    HeartbeatNotDue { age_secs: u64 },
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimited { elapsed, interval } => write!(
                f,
                "rate limited: last publish {elapsed}s ago, minimum interval {interval}s"
            ),
            Self::ValueUnchanged => write!(f, "on-chain value already equals the measurement"),
            Self::HeartbeatNotDue { age_secs } => write!(
                f,
                "heartbeat not due: v1 beacon is {age_secs}s old, minimum {}s",
                v1::HEARTBEAT_MIN_AGE_SECS
            ),
        }
    }
}

/// Outcome of an ECDSA beacon update.
pub enum EcdsaUpdateOutcome {
    /// An update transaction was sent. `confirmed == false` means its receipt
    /// did not arrive within the wait window — it may still confirm on-chain.
    /// The route surfaces this to the caller (the Python updater) instead of
    /// erroring, so the caller can poll the hash rather than blindly
    /// re-sending. `beacon_address` lets the route dispatch a follow-up touch
    /// of the perps backed by this beacon (only when `confirmed`).
    Published {
        tx_hash: B256,
        confirmed: bool,
        beacon_address: Address,
    },
    /// No transaction was sent. Either the beacon is inside its publish rate
    /// limit, or the on-chain index already equals the requested measurement
    /// and the last publish is inside the dedupe heartbeat window (see
    /// [`super::dedupe`]). `reason` is for logging only — the route reports
    /// every skip to the updater the same way.
    Skipped {
        beacon_address: Address,
        reason: SkipReason,
    },
}

/// The publish rate limit (step 4a / 4a-bis of [`update_beacon_with_ecdsa`]):
/// `Some(Skipped)` when the last publish of this beacon is newer than its
/// interval. A missing record (no Redis, first publish, expired key) is `None`.
async fn rate_limit_skip(
    state: &AppState,
    beacon_address: Address,
    tier: Option<Tier>,
) -> Option<EcdsaUpdateOutcome> {
    let interval = super::dedupe::publish_interval(&beacon_address, tier)?;
    let elapsed = super::dedupe::seconds_since_last_publish(state, &beacon_address).await?;
    if elapsed >= interval.as_secs() {
        return None;
    }
    let reason = SkipReason::RateLimited {
        elapsed,
        interval: interval.as_secs(),
    };
    tracing::info!(
        tier = tier.map(|t| t.as_str()).unwrap_or("unknown"),
        "Skipping update for beacon {beacon_address}: {reason}"
    );
    Some(EcdsaUpdateOutcome::Skipped {
        beacon_address,
        reason,
    })
}

/// Updates a beacon using ECDSA signature from the PRIVATE_KEY wallet.
///
/// This function:
/// 1. Gets the verifier address from the beacon via `verifier()`
/// 2. Gets the designated signer from the ECDSAVerifier via `SIGNER()`
/// 3. Verifies PRIVATE_KEY signer matches designated signer
/// 4. Acquires the per-beacon update lock (serializes verifier-nonce use), reads
///    `index()` once to learn the beacon kind (old or v1) and, for v1, its age
/// 5. Applies the v1 age gate, the rate limit and the value dedupe
/// 6. Builds the nonce: a timestamp (old kind) or `lastNonce + 1` (v1)
/// 7. Gets the EIP-712 digest from the verifier via `digest(uint256[], uint256)`
/// 8. Signs the digest with PRIVATE_KEY signer
/// 9. Packs the signature as r || s || v (65 bytes)
/// 10. ABI-encodes the inputs as (uint256[] measurement, uint256 nonce)
/// 11. Calls beacon.update(signature, inputs)
pub async fn update_beacon_with_ecdsa(
    state: &AppState,
    request: UpdateBeaconWithEcdsaRequest,
) -> Result<EcdsaUpdateOutcome, String> {
    // 1. Parse beacon address and measurement(s)
    let beacon_address = Address::from_str(&request.beacon_address)
        .map_err(|e| format!("Invalid beacon address: {e}"))?;

    let measurement_array: Vec<U256> = request
        .measurement
        .iter()
        .enumerate()
        .map(|(i, s)| {
            U256::from_str(s).map_err(|e| format!("Invalid measurement value at index {i}: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if measurement_array.is_empty() {
        return Err("Measurement array must not be empty".to_string());
    }

    tracing::info!(
        "Updating beacon {} with ECDSA-signed measurement ({} element(s)): {:?}",
        beacon_address,
        measurement_array.len(),
        measurement_array
    );

    // 2. Get verifier address from beacon using read provider
    let beacon_read = IBeacon::new(beacon_address, &*state.provider.read_provider);
    let verifier_address_raw = beacon_read
        .verifier()
        .call()
        .await
        .map_err(|e| format!("Failed to get verifier address: {e}"))?;
    let verifier_address = Address::from(verifier_address_raw.0);

    tracing::info!("Beacon verifier: {}", verifier_address);

    // Get the designated signer from the verifier using read provider
    let verifier = IEcdsaVerifier::new(verifier_address, &*state.provider.read_provider);
    let designated_signer_raw = verifier
        .SIGNER()
        .call()
        .await
        .map_err(|e| format!("Failed to get designated signer: {e}"))?;
    let designated_signer = Address::from(designated_signer_raw.0);

    tracing::info!("Designated signer for this beacon: {}", designated_signer);

    // 3. Verify PRIVATE_KEY signer matches designated signer
    let signer_address = state.wallets.signer.address();
    if signer_address != designated_signer {
        return Err(format!(
            "PRIVATE_KEY wallet {signer_address} does not match designated signer {designated_signer} for beacon {beacon_address}. \
             Update PRIVATE_KEY or reconfigure the beacon's verifier."
        ));
    }

    tracing::info!(
        "Using PRIVATE_KEY signer {} for beacon {} ECDSA signature",
        signer_address,
        beacon_address
    );

    // 4. Serialize updates per beacon BEFORE taking a pool wallet: the verifier
    // nonce is per-beacon, and two in-flight updates (via different gas payers)
    // can land out of nonce order, reverting the loser on-chain. The guard is
    // held through receipt-wait so the next update reads post-landing state.
    // Lock order is always beacon -> wallet, so no deadlock is possible; taking
    // the beacon lock first also avoids parking a pool wallet while queued.
    let beacon_update_lock = state
        .wallets
        .manager
        .acquire_beacon_update_lock(beacon_address)
        .await?;

    // 4a. Publish rate limit for a beacon already known to be old: checked
    // before the index() read, as before v1, so a rate-limited update of an
    // old beacon costs no RPC call. Other beacons are checked in 4a-bis, after
    // the read tells their kind.
    let cached_kind = v1::known_kind(beacon_address);
    let tier = state.touch.cached_tier(beacon_address).await;
    if cached_kind == Some(VerifierKind::Legacy)
        && let Some(skipped) = rate_limit_skip(state, beacon_address, tier).await
    {
        return Ok(skipped);
    }

    // 4a. Read index() once, under the lock. The reply length is the beacon kind
    // (see services/beacon/v1.rs). A v1 beacon needs the read for its age.
    // When the read fails, only a beacon known to be v1 stops here. Any other
    // beacon publishes as the old kind, as before v1, so an RPC error after a
    // restart (empty kind cache) never stops pushes to the old beacons. A v1
    // beacon not yet in the cache then fails on its verifier nonce, which is
    // the same result as stopping here.
    let reading: Option<IndexReading> = match v1::read_index(
        &*state.provider.read_provider,
        beacon_address,
    )
    .await
    {
        Ok(r) => {
            v1::remember_kind(beacon_address, r.kind);
            Some(r)
        }
        Err(e) if cached_kind == Some(VerifierKind::V1) => {
            return Err(format!("Cannot read the v1 beacon {beacon_address}: {e}"));
        }
        Err(e) => {
            tracing::warn!(
                "{e} — publishing beacon {beacon_address} as the old kind, without the value dedupe"
            );
            None
        }
    };
    let kind = reading.map_or(VerifierKind::Legacy, |r| r.kind);

    // 4a-v1. Age gate for v1 beacons. The heartbeat runs here, under the
    // per-beacon update lock, so it is ordered with real updates: a heartbeat
    // that queues behind a real update reads the new age and skips.
    let v1_gate = match reading {
        Some(IndexReading {
            updated_at: Some(updated_at),
            ..
        }) => Some(v1::gate(
            request.heartbeat,
            v1::age_secs(updated_at, v1::unix_now_secs()?),
        )),
        _ => None,
    };
    if let Some(V1Gate::SkipHeartbeat { age_secs }) = v1_gate {
        let reason = SkipReason::HeartbeatNotDue { age_secs };
        tracing::info!("Skipping update for beacon {beacon_address}: {reason}");
        return Ok(EcdsaUpdateOutcome::Skipped {
            beacon_address,
            reason,
        });
    }
    let publish_now = matches!(v1_gate, Some(V1Gate::PublishNow { .. }));
    if let Some(V1Gate::PublishNow { age_secs }) = v1_gate {
        tracing::info!(
            "v1 beacon {beacon_address} is {age_secs}s old: publishing without the rate limit and value dedupe"
        );
    }

    // 4a-bis. Publish rate limit: a floor on time between publishes, applied
    // whether or not the value changed. It costs only the 4a index() read
    // (needed to know the kind), and it runs before any wallet is acquired so
    // it never parks a pool wallet. A v1 beacon of 900 s or more skips this
    // gate (`publish_now`): a stale v1 beacon stops its market. A missing record
    // (no Redis, first publish, expired key) falls through and publishes.
    // The tier read is cache-only (see TouchDispatcher::cached_tier): a cold
    // cache or disabled touch yields None and falls through to the configured
    // default, never a bot-api round trip under the beacon lock.
    if !publish_now
        && cached_kind != Some(VerifierKind::Legacy)
        && let Some(skipped) = rate_limit_skip(state, beacon_address, tier).await
    {
        return Ok(skipped);
    }

    // 4b. Value dedupe: when the on-chain index already equals the requested
    // measurement, skip the tx unless the heartbeat window has elapsed. It uses
    // the 4a read, taken under the beacon update lock, so it reflects the
    // previous update. Every failure here falls through to a normal publish.
    if !publish_now
        && let Some(window) = super::dedupe::heartbeat_window()
        && measurement_array.len() == 1
        && let Some(current) = reading.map(|r| r.index)
        && current == measurement_array[0]
        && let Some(elapsed) =
            super::dedupe::seconds_since_last_publish(state, &beacon_address).await
        && elapsed < window.as_secs()
    {
        tracing::info!(
            "Skipping update for beacon {beacon_address}: on-chain index \
             already {current}, last publish {elapsed}s ago (heartbeat {}s)",
            window.as_secs()
        );
        return Ok(EcdsaUpdateOutcome::Skipped {
            beacon_address,
            reason: SkipReason::ValueUnchanged,
        });
    }

    // 5. Bounded retry budget for wallet acquisition: one attempt per pool
    // wallet at most (floor 1). A wallet that turns out to be drained of gas
    // (insufficient funds on preflight or send, see step 11/12) is excluded
    // and a different one is tried instead of failing the whole update — this
    // is what stops a single drained wallet from freezing every update, as
    // happened on 2026-06-30 when selection had no such fallback.
    let max_wallet_attempts = state.wallets.manager.signer_addresses().len().max(1);
    let mut excluded_wallets: std::collections::HashSet<Address> = std::collections::HashSet::new();

    // 6. Nonce. Old kind: a high-resolution timestamp (any unused proof is
    // accepted). v1: exactly lastNonce + 1, read under the beacon update lock
    // so no other update of this beacon can take the same nonce.
    let nonce = match kind {
        VerifierKind::Legacy => U256::from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| format!("Failed to get system time: {e}"))?
                .as_nanos(),
        ),
        VerifierKind::V1 => {
            let last =
                v1::read_last_nonce(&*state.provider.read_provider, verifier_address).await?;
            v1::nonce_for_update(verifier_address, last)?
        }
    };

    tracing::info!("Using nonce ({kind:?} kind): {}", nonce);

    // 7. Get EIP-712 digest from verifier (measurement is uint256[])
    let digest_raw = verifier
        .digest(measurement_array.clone(), nonce)
        .call()
        .await
        .map_err(|e| format!("Failed to get EIP-712 digest: {e}"))?;
    let digest = B256::from(digest_raw.0);

    tracing::info!("Got EIP-712 digest: {:?}", digest);

    // 8. Sign the digest with PRIVATE_KEY signer (state.wallets.signer)
    let signature = state
        .wallets
        .signer
        .sign_hash(&digest)
        .await
        .map_err(|e| format!("Failed to sign digest with PRIVATE_KEY signer: {e}"))?;

    tracing::info!("Signed digest successfully");

    // 9. Pack signature as r || s || v (65 bytes)
    // Alloy signature.as_bytes() returns [r (32 bytes) | s (32 bytes) | v (1 byte)]
    let sig_bytes = Bytes::from(signature.as_bytes().to_vec());

    let proof_hash = alloy::primitives::keccak256(sig_bytes.as_ref());

    tracing::info!(
        "Signature: len={}, proof_hash={:#x}",
        sig_bytes.len(),
        proof_hash
    );
    tracing::debug!(
        "Signature details: v={}, r={:#x}, s={:#x}",
        if signature.v() { 28u8 } else { 27u8 },
        signature.r(),
        signature.s()
    );

    // 10. ABI-encode inputs as (uint256[] measurement, uint256 nonce)
    // Use abi_encode_params to match Solidity's abi.encode(uint256[], uint256)
    let inputs = <(
        alloy::sol_types::sol_data::Array<alloy::sol_types::sol_data::Uint<256>>,
        alloy::sol_types::sol_data::Uint<256>,
    )>::abi_encode_params(&(measurement_array.clone(), nonce));
    let inputs_bytes = Bytes::from(inputs);
    let inputs_hash = alloy::primitives::keccak256(inputs_bytes.as_ref());

    tracing::info!(
        "Update params: proof_len={}, inputs_len={}, inputs_hash={:#x}",
        sig_bytes.len(),
        inputs_bytes.len(),
        inputs_hash
    );
    tracing::debug!(
        "Update raw values: measurement={:?}, nonce={}",
        measurement_array,
        nonce
    );

    // 11 + 12. Acquire a wallet, simulate, and send — retrying with a
    // different (excluded) wallet on an insufficient-funds error from either
    // step. Any other error returns immediately, exactly as before this loop
    // existed. The lock guarding one wallet is dropped before the next
    // attempt acquires a different one; the per-beacon `beacon_update_lock`
    // stays held across every attempt (it was acquired before this loop, in
    // step 4, and spans the whole function).
    let mut wallet_handle: Option<WalletHandle> = None;
    let mut pending_tx = None;

    for attempt in 1..=max_wallet_attempts {
        let handle = state
            .wallets
            .manager
            .acquire_any_wallet_excluding(&excluded_wallets)
            .await
            .map_err(|e| format!("Failed to acquire wallet for transaction: {e}"))?;
        let attempt_address = handle.address();
        tracing::info!(
            "Using wallet {} from pool to send transaction (gas payer) [attempt {attempt}/{max_wallet_attempts}]",
            attempt_address
        );

        // Build provider with the acquired wallet for sending transactions
        let provider = handle
            .build_provider(&state.provider.rpc_url)
            .map_err(|e| format!("Failed to build provider: {e}"))?;

        // 11. Simulate the update call first to get revert reason if it would fail
        let beacon_write = IBeacon::new(beacon_address, &provider);
        if let Err(e) = beacon_write
            .update(sig_bytes.clone(), inputs_bytes.clone())
            .call()
            .await
        {
            // Only run diagnostics for EVM reverts, not transport/network failures
            let error_msg = if e.as_revert_data().is_some() {
                let diag_timeout = Duration::from_secs(5);

                let verify_detail = match timeout(
                    diag_timeout,
                    verifier
                        .verify(sig_bytes.clone(), inputs_bytes.clone())
                        .call(),
                )
                .await
                {
                    Ok(Ok(_)) => "verify() succeeded".to_string(),
                    Ok(Err(ve)) => format!("verify() also reverted: {ve}"),
                    Err(_) => "verify() diagnostic timed out".to_string(),
                };

                let used_detail = match kind {
                    VerifierKind::Legacy => {
                        match timeout(diag_timeout, verifier.usedProofs(proof_hash).call()).await {
                            Ok(Ok(val)) => {
                                format!("usedProofs({proof_hash:#x})={val:?}")
                            }
                            Ok(Err(ue)) => format!("usedProofs check failed: {ue}"),
                            Err(_) => "usedProofs diagnostic timed out".to_string(),
                        }
                    }
                    VerifierKind::V1 => match timeout(
                        diag_timeout,
                        v1::read_last_nonce(&*state.provider.read_provider, verifier_address),
                    )
                    .await
                    {
                        Ok(Ok(last)) => format!("lastNonce()={last}, signed nonce {nonce}"),
                        Ok(Err(ne)) => ne,
                        Err(_) => "lastNonce diagnostic timed out".to_string(),
                    },
                };

                format!(
                    "Preflight simulation of beacon.update() reverted: {e}. \
                     Verifier diagnostics: {verify_detail}. {used_detail}"
                )
            } else {
                // Transport or other non-revert error: fail fast without diagnostics
                format!("Preflight simulation of beacon.update() failed (transport/RPC): {e}")
            };
            tracing::error!("{}", error_msg);

            if is_insufficient_funds_error(&error_msg) {
                // Process-wide cooldown, not just this request's exclusion set:
                // without it a fully drained pool re-sweeps every wallet on
                // every incoming update (the 2026-08-24..26 RPC burn storm).
                state.wallets.manager.mark_drained(attempt_address);
                if attempt < max_wallet_attempts {
                    tracing::warn!(
                        "Wallet {attempt_address} appears out of gas (preflight simulation failed with \
                         insufficient funds); excluding it and retrying with a different pool wallet"
                    );
                    excluded_wallets.insert(attempt_address);
                    drop(handle);
                    continue;
                }
            }
            return Err(error_msg);
        }
        tracing::info!("Preflight simulation of beacon.update() succeeded");

        // 12. Send the actual transaction
        tracing::info!(
            "Sending update transaction to beacon with wallet {}",
            attempt_address
        );
        // Nonce errors are retried inside the helper; only non-nonce errors
        // (e.g. a drained wallet) surface here for the rotation below.
        let send_result = send_with_nonce_retry("ecdsa beacon update", || {
            let handle = &handle;
            let call = beacon_write.update(sig_bytes.clone(), inputs_bytes.clone());
            Box::pin(async move {
                handle.ensure_lock_held()?;
                call.send()
                    .await
                    .map_err(|e| format!("Failed to send update transaction: {e}"))
            })
        })
        .await;
        match send_result {
            Ok(pt) => {
                pending_tx = Some(pt);
                wallet_handle = Some(handle);
                break;
            }
            Err(error_msg) => {
                if is_insufficient_funds_error(&error_msg) {
                    // Process-wide cooldown, not just this request's exclusion
                    // set: without it a fully drained pool re-sweeps every
                    // wallet on every incoming update (2026-08-24..26 storm).
                    state.wallets.manager.mark_drained(attempt_address);
                    if attempt < max_wallet_attempts {
                        tracing::warn!(
                            "Wallet {attempt_address} appears out of gas (send failed with \
                             insufficient funds); excluding it and retrying with a different pool wallet"
                        );
                        excluded_wallets.insert(attempt_address);
                        drop(handle);
                        continue;
                    }
                }
                return Err(error_msg);
            }
        }
    }

    // The loop above always either returns early or `break`s with both `Some`.
    // `_wallet_handle` is intentionally never read again: its only remaining
    // job is to keep the winning wallet's distributed lock held (via Drop)
    // for the rest of this function, exactly like the un-looped version did.
    let _wallet_handle = wallet_handle
        .expect("acquire/simulate/send retry loop must return or break with a wallet handle");
    let pending_tx = pending_tx
        .expect("acquire/simulate/send retry loop must return or break with a pending tx");

    tracing::info!("Transaction sent, waiting for receipt...");

    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("Transaction hash: {:?}", tx_hash);

    // 13. Wait for confirmation with timeout. The tx WAS sent either way; on no
    // receipt it may still land, so the caller gets the hash as unconfirmed.
    let receipt = match timeout(Duration::from_secs(60), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => {
            tracing::info!("Transaction confirmed via get_receipt()");
            receipt
        }
        not_confirmed => {
            let why = match not_confirmed {
                Ok(Err(e)) => format!("receipt fetch failed: {e}"),
                _ => "timeout waiting for the receipt".to_string(),
            };
            return Ok(handle_unconfirmed(
                state,
                kind,
                request.heartbeat,
                verifier_address,
                nonce,
                beacon_update_lock,
                tx_hash,
                beacon_address,
                &why,
            )
            .await);
        }
    };

    // 14. Validate transaction status
    if !receipt.status() {
        let error_msg = format!("update() transaction {tx_hash} reverted (status: false)");
        tracing::error!("{}", error_msg);
        tracing::error!("Receipt: {:?}", receipt);
        return Err(error_msg);
    }

    // 15. Validate IndexUpdated event was emitted
    let index_updated_found = receipt.inner.logs().iter().any(|log| {
        // IndexUpdated event signature: keccak256("IndexUpdated(uint256)")
        log.address() == beacon_address
            && !log.topics().is_empty()
            && log.topics()[0] == alloy::primitives::keccak256("IndexUpdated(uint256)")
    });

    if index_updated_found {
        tracing::info!(
            "ECDSA beacon update succeeded - beacon {} updated with measurement ({} element(s))",
            beacon_address,
            measurement_array.len()
        );
    } else {
        // Transaction succeeded but event not found - still consider it a success
        // as the transaction was confirmed
        tracing::warn!(
            "Transaction succeeded but IndexUpdated event not found for beacon {}. \
             The update may have been applied but event parsing failed.",
            beacon_address
        );
    }
    if kind == VerifierKind::V1 {
        v1::record_confirmed_nonce(verifier_address, nonce);
    }
    if records_publish(kind, request.heartbeat) {
        super::dedupe::record_publish(state, &beacon_address).await;
    }
    Ok(EcdsaUpdateOutcome::Published {
        tx_hash,
        confirmed: true,
        beacon_address,
    })
}

/// A sent update has no receipt yet.
///
/// Old kind (unchanged): keep the beacon lock until the receipt, record the
/// publish, report unconfirmed.
///
/// v1: never record an unconfirmed publish as done. Re-read `lastNonce`: if
/// the verifier has accepted our nonce, the update is on chain, so report it
/// as confirmed. If not, keep the beacon lock until the receipt (so no second
/// update takes the same nonce) and do not record the publish, so the next
/// request is not deduped or rate limited against an update that may never
/// land.
#[allow(clippy::too_many_arguments)]
async fn handle_unconfirmed(
    state: &AppState,
    kind: VerifierKind,
    heartbeat: bool,
    verifier_address: Address,
    nonce: U256,
    beacon_update_lock: (LockHeartbeat, WalletLockGuard),
    tx_hash: B256,
    beacon_address: Address,
    why: &str,
) -> EcdsaUpdateOutcome {
    if kind == VerifierKind::V1 {
        match v1::read_last_nonce(&*state.provider.read_provider, verifier_address).await {
            Ok(last) if v1::landed(last, nonce) => {
                tracing::info!(
                    "Update tx {tx_hash} for v1 beacon {beacon_address}: {why}, but lastNonce {last} >= {nonce}; it is on chain"
                );
                v1::record_confirmed_nonce(verifier_address, nonce);
                if records_publish(kind, heartbeat) {
                    super::dedupe::record_publish(state, &beacon_address).await;
                }
                drop(beacon_update_lock);
                return EcdsaUpdateOutcome::Published {
                    tx_hash,
                    confirmed: true,
                    beacon_address,
                };
            }
            Ok(last) => tracing::warn!(
                "Update tx {tx_hash} for v1 beacon {beacon_address}: {why}; lastNonce {last} < {nonce}, not on chain yet"
            ),
            Err(e) => {
                tracing::warn!("Update tx {tx_hash} for v1 beacon {beacon_address}: {why}; {e}")
            }
        }
    } else {
        tracing::warn!(
            "Update tx {tx_hash} for beacon {beacon_address}: {why} - returning unconfirmed"
        );
        super::dedupe::record_publish(state, &beacon_address).await;
    }
    hold_beacon_lock_until_receipt(
        beacon_update_lock,
        state.provider.read_provider.clone(),
        tx_hash,
        beacon_address,
        (kind == VerifierKind::V1).then_some((verifier_address, nonce)),
    );
    EcdsaUpdateOutcome::Published {
        tx_hash,
        confirmed: false,
        beacon_address,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_v1_heartbeat_does_not_feed_the_rate_limit() {
        assert!(!records_publish(VerifierKind::V1, true));
        assert!(records_publish(VerifierKind::V1, false));
        assert!(records_publish(VerifierKind::Legacy, true));
        assert!(records_publish(VerifierKind::Legacy, false));
    }

    #[test]
    fn test_abi_encode_inputs() {
        let measurement = U256::from(1000000000000000000u128);
        let nonce = U256::from(1704067200u64); // Example timestamp

        let measurement_array = vec![measurement];
        // Use abi_encode_params (flat params encoding) to match Solidity's abi.decode(inputs, (uint256[], uint256))
        let inputs = <(
            alloy::sol_types::sol_data::Array<alloy::sol_types::sol_data::Uint<256>>,
            alloy::sol_types::sol_data::Uint<256>,
        )>::abi_encode_params(&(measurement_array, nonce));

        // Flat params encoding: offset(32) + nonce(32) + length(32) + element(32) = 128 bytes
        assert_eq!(inputs.len(), 128);
    }

    #[test]
    fn test_abi_encode_inputs_multi_element() {
        let measurements = vec![
            U256::from(47941000000000000u128),
            U256::from(226802000000000000u128),
            U256::from(354746000000000000u128),
        ];
        let nonce = U256::from(1704067200u64);

        let inputs = <(
            alloy::sol_types::sol_data::Array<alloy::sol_types::sol_data::Uint<256>>,
            alloy::sol_types::sol_data::Uint<256>,
        )>::abi_encode_params(&(measurements, nonce));

        // offset(32) + nonce(32) + length(32) + 3 elements(96) = 192 bytes
        assert_eq!(inputs.len(), 192);
    }

    #[test]
    fn test_parse_measurement() {
        // Test decimal string parsing
        let result = U256::from_str("1000000000000000000");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), U256::from(1000000000000000000u128));
    }
}
