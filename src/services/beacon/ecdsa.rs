use alloy::primitives::{Address, B256, Bytes, U256};
use alloy::providers::Provider;
use alloy::signers::Signer;
use alloy::sol_types::SolType;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::timeout;
use tracing;

use crate::ReadOnlyProvider;
use crate::models::{AppState, UpdateBeaconWithEcdsaRequest};
use crate::routes::{IBeacon, IEcdsaVerifier};
use crate::services::wallet::{LockHeartbeat, WalletLockGuard};

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
fn hold_beacon_lock_until_receipt(
    lock: (LockHeartbeat, WalletLockGuard),
    provider: Arc<ReadOnlyProvider>,
    tx_hash: B256,
    beacon_address: Address,
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
                Ok(Ok(Some(_))) => {
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

/// Outcome of an ECDSA beacon update.
///
/// `confirmed == false` means the transaction was SENT but its receipt did not
/// arrive within the wait window — it may still confirm on-chain. The route
/// surfaces this to the caller (the Python updater) instead of erroring, so the
/// caller can poll the hash rather than blindly re-sending.
pub struct EcdsaUpdateOutcome {
    pub tx_hash: B256,
    pub confirmed: bool,
}

/// Updates a beacon using ECDSA signature from the PRIVATE_KEY wallet.
///
/// This function:
/// 1. Gets the verifier address from the beacon via `verifier()`
/// 2. Gets the designated signer from the ECDSAVerifier via `SIGNER()`
/// 3. Verifies PRIVATE_KEY signer matches designated signer
/// 4. Acquires the per-beacon update lock (serializes verifier-nonce use)
/// 5. Acquires any available wallet from pool for transaction sending
/// 6. Generates a nonce from the current timestamp
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

    // 5. Acquire any available wallet from pool for sending the transaction
    let wallet_handle = state
        .wallets
        .manager
        .acquire_any_wallet()
        .await
        .map_err(|e| format!("Failed to acquire wallet for transaction: {e}"))?;

    let tx_wallet_address = wallet_handle.address();
    tracing::info!(
        "Using wallet {} from pool to send transaction (gas payer)",
        tx_wallet_address
    );

    // Build provider with the acquired wallet for sending transactions
    let provider = wallet_handle
        .build_provider(&state.provider.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;

    // 6. Generate nonce from high-resolution timestamp (nanoseconds) to avoid collisions
    let nonce = U256::from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("Failed to get system time: {e}"))?
            .as_nanos(),
    );

    tracing::info!("Using nonce (timestamp_nanos): {}", nonce);

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

    // 11. Simulate the update call first to get revert reason if it would fail
    let beacon_write = IBeacon::new(beacon_address, &provider);
    match beacon_write
        .update(sig_bytes.clone(), inputs_bytes.clone())
        .call()
        .await
    {
        Ok(_) => {
            tracing::info!("Preflight simulation of beacon.update() succeeded");
        }
        Err(e) => {
            // Only run diagnostics for EVM reverts, not transport/network failures
            if e.as_revert_data().is_some() {
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

                let used_detail =
                    match timeout(diag_timeout, verifier.usedProofs(proof_hash).call()).await {
                        Ok(Ok(val)) => {
                            format!("usedProofs({proof_hash:#x})={val:?}")
                        }
                        Ok(Err(ue)) => format!("usedProofs check failed: {ue}"),
                        Err(_) => "usedProofs diagnostic timed out".to_string(),
                    };

                let error_msg = format!(
                    "Preflight simulation of beacon.update() reverted: {e}. \
                     Verifier diagnostics: {verify_detail}. {used_detail}"
                );
                tracing::error!("{}", error_msg);
                return Err(error_msg);
            }

            // Transport or other non-revert error: fail fast without diagnostics
            let error_msg =
                format!("Preflight simulation of beacon.update() failed (transport/RPC): {e}");
            tracing::error!("{}", error_msg);
            return Err(error_msg);
        }
    }

    // 12. Send the actual transaction
    tracing::info!(
        "Sending update transaction to beacon with wallet {}",
        tx_wallet_address
    );
    wallet_handle.ensure_lock_held()?;
    let pending_tx = beacon_write
        .update(sig_bytes.clone(), inputs_bytes.clone())
        .send()
        .await
        .map_err(|e| format!("Failed to send update transaction: {e}"))?;

    tracing::info!("Transaction sent, waiting for receipt...");

    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("Transaction hash: {:?}", tx_hash);

    // 13. Wait for confirmation with timeout
    let receipt = match timeout(Duration::from_secs(60), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => {
            tracing::info!("Transaction confirmed via get_receipt()");
            receipt
        }
        Ok(Err(e)) => {
            // The tx WAS sent; the receipt fetch failed but it may still land.
            // Keep the beacon lock alive in the background so no second update
            // races the pending one on the verifier nonce, and return the hash
            // as sent-but-unconfirmed so the caller polls instead of re-sending.
            hold_beacon_lock_until_receipt(
                beacon_update_lock,
                state.provider.read_provider.clone(),
                tx_hash,
                beacon_address,
            );
            tracing::error!(
                "Failed to get receipt for sent update tx {tx_hash}: {e} - returning unconfirmed"
            );
            return Ok(EcdsaUpdateOutcome {
                tx_hash,
                confirmed: false,
            });
        }
        Err(_) => {
            // The transaction WAS sent; it may still confirm. Report it as
            // sent-but-unconfirmed so the caller can poll instead of re-sending,
            // and keep the beacon lock alive until the tx resolves so a retry
            // cannot race it on the verifier nonce.
            hold_beacon_lock_until_receipt(
                beacon_update_lock,
                state.provider.read_provider.clone(),
                tx_hash,
                beacon_address,
            );
            tracing::warn!(
                "Timeout waiting for transaction {tx_hash} receipt — returning unconfirmed"
            );
            sentry::capture_message(
                &format!("ECDSA beacon update sent but unconfirmed at timeout (tx {tx_hash})"),
                sentry::Level::Warning,
            );
            return Ok(EcdsaUpdateOutcome {
                tx_hash,
                confirmed: false,
            });
        }
    };

    // 14. Validate transaction status
    if !receipt.status() {
        let error_msg = format!("update() transaction {tx_hash} reverted (status: false)");
        tracing::error!("{}", error_msg);
        tracing::error!("Receipt: {:?}", receipt);
        sentry::capture_message(&error_msg, sentry::Level::Error);
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
        Ok(EcdsaUpdateOutcome {
            tx_hash,
            confirmed: true,
        })
    } else {
        // Transaction succeeded but event not found - still consider it a success
        // as the transaction was confirmed
        tracing::warn!(
            "Transaction succeeded but IndexUpdated event not found for beacon {}. \
             The update may have been applied but event parsing failed.",
            beacon_address
        );
        Ok(EcdsaUpdateOutcome {
            tx_hash,
            confirmed: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
