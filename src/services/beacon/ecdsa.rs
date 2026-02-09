use alloy::primitives::{Address, B256, Bytes, U256};
use alloy::signers::Signer;
use alloy::sol_types::SolValue;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::timeout;
use tracing;

use crate::models::{AppState, UpdateBeaconWithEcdsaRequest};
use crate::routes::{IEcdsaBeacon, IEcdsaVerifierAdapter};

/// Updates a beacon using ECDSA signature from the appropriate wallet.
///
/// This function:
/// 1. Gets the verifier adapter address from the beacon
/// 2. Gets the designated signer from the verifier adapter
/// 3. Acquires the appropriate wallet (via WalletManager if available, or fallback to single wallet)
/// 4. Generates a nonce from the current timestamp
/// 5. Gets the EIP-712 digest from the verifier
/// 6. Signs the digest with the acquired wallet
/// 7. Packs the signature as r || s || v (65 bytes)
/// 8. ABI-encodes the inputs as (measurement, nonce)
/// 9. Calls beacon.updateIndex(signature, inputs)
pub async fn update_beacon_with_ecdsa(
    state: &AppState,
    request: UpdateBeaconWithEcdsaRequest,
) -> Result<B256, String> {
    // 1. Parse beacon address and measurement
    let beacon_address = Address::from_str(&request.beacon_address)
        .map_err(|e| format!("Invalid beacon address: {e}"))?;

    let measurement = U256::from_str(&request.measurement)
        .map_err(|e| format!("Invalid measurement value: {e}"))?;

    tracing::info!(
        "Updating beacon {} with ECDSA-signed measurement: {}",
        beacon_address,
        measurement
    );

    // 2. Get verifier adapter address from beacon using read provider
    let beacon_read = IEcdsaBeacon::new(beacon_address, &*state.read_provider);
    let verifier_address_raw = beacon_read
        .verifierAdapter()
        .call()
        .await
        .map_err(|e| format!("Failed to get verifier adapter address: {e}"))?;
    let verifier_address = Address::from(verifier_address_raw.0);

    tracing::info!("Beacon verifier adapter: {}", verifier_address);

    // Get the designated signer from the verifier adapter using read provider
    let verifier = IEcdsaVerifierAdapter::new(verifier_address, &*state.read_provider);
    let designated_signer_raw = verifier
        .SIGNER()
        .call()
        .await
        .map_err(|e| format!("Failed to get designated signer: {e}"))?;
    let designated_signer = Address::from(designated_signer_raw.0);

    tracing::info!("Designated signer for this beacon: {}", designated_signer);

    // 3. Verify PRIVATE_KEY signer matches designated signer
    let signer_address = state.signer.address();
    if signer_address != designated_signer {
        return Err(format!(
            "PRIVATE_KEY wallet {signer_address} does not match designated signer {designated_signer} for beacon {beacon_address}. \
             Update PRIVATE_KEY or reconfigure the beacon's verifier adapter."
        ));
    }

    tracing::info!(
        "Using PRIVATE_KEY signer {} for beacon {} ECDSA signature",
        signer_address,
        beacon_address
    );

    // 4. Acquire any available wallet from pool for sending the transaction
    let wallet_handle = state
        .wallet_manager
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
        .build_provider(&state.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;

    // 4. Generate nonce from high-resolution timestamp (nanoseconds) to avoid collisions
    let nonce = U256::from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("Failed to get system time: {e}"))?
            .as_nanos(),
    );

    tracing::info!("Using nonce (timestamp_nanos): {}", nonce);

    // 5. Get EIP-712 digest from verifier
    let digest_raw = verifier
        .digest(measurement, nonce)
        .call()
        .await
        .map_err(|e| format!("Failed to get EIP-712 digest: {e}"))?;
    let digest = B256::from(digest_raw.0);

    tracing::info!("Got EIP-712 digest: {:?}", digest);

    // 6. Sign the digest with PRIVATE_KEY signer (state.signer)
    let signature = state
        .signer
        .sign_hash(&digest)
        .await
        .map_err(|e| format!("Failed to sign digest with PRIVATE_KEY signer: {e}"))?;

    tracing::info!("Signed digest successfully");

    // 7. Pack signature as r || s || v (65 bytes)
    // Alloy signature.as_bytes() returns [r (32 bytes) | s (32 bytes) | v (1 byte)]
    let sig_bytes = Bytes::from(signature.as_bytes().to_vec());

    tracing::debug!("Signature bytes length: {}", sig_bytes.len());

    // 8. ABI-encode inputs as (measurement, nonce)
    let inputs = (measurement, nonce).abi_encode();
    let inputs_bytes = Bytes::from(inputs);

    tracing::debug!("Inputs bytes length: {}", inputs_bytes.len());

    // 9. Call beacon.updateIndex(signature, inputs) using the write provider
    tracing::info!(
        "Sending updateIndex transaction to beacon with wallet {}",
        tx_wallet_address
    );
    let beacon_write = IEcdsaBeacon::new(beacon_address, &provider);
    let pending_tx = beacon_write
        .updateIndex(sig_bytes.clone(), inputs_bytes.clone())
        .send()
        .await
        .map_err(|e| format!("Failed to send updateIndex transaction: {e}"))?;

    tracing::info!("Transaction sent, waiting for receipt...");

    // Get the transaction hash before calling get_receipt()
    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("Transaction hash: {:?}", tx_hash);

    // 10. Wait for confirmation with timeout
    let receipt = match timeout(Duration::from_secs(60), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => {
            tracing::info!("Transaction confirmed via get_receipt()");
            receipt
        }
        Ok(Err(e)) => {
            let error_msg = format!("Failed to get transaction receipt: {e}");
            tracing::error!("{}", error_msg);
            return Err(error_msg);
        }
        Err(_) => {
            let error_msg = format!("Timeout waiting for transaction {tx_hash} receipt");
            tracing::error!("{}", error_msg);
            return Err(error_msg);
        }
    };

    // 11. Validate transaction status
    if !receipt.status() {
        let error_msg = format!("UpdateIndex transaction {tx_hash} reverted (status: false)");
        tracing::error!("{}", error_msg);
        tracing::error!("Receipt: {:?}", receipt);
        sentry::capture_message(&error_msg, sentry::Level::Error);
        return Err(error_msg);
    }

    // 12. Validate IndexUpdated event was emitted
    let index_updated_found = receipt.inner.logs().iter().any(|log| {
        // IndexUpdated event signature: keccak256("IndexUpdated(uint256)")
        log.address() == beacon_address
            && !log.topics().is_empty()
            && log.topics()[0] == alloy::primitives::keccak256("IndexUpdated(uint256)")
    });

    if index_updated_found {
        tracing::info!(
            "ECDSA beacon update succeeded - beacon {} updated with measurement {}",
            beacon_address,
            measurement
        );
        Ok(tx_hash)
    } else {
        // Transaction succeeded but event not found - still consider it a success
        // as the transaction was confirmed
        tracing::warn!(
            "Transaction succeeded but IndexUpdated event not found for beacon {}. \
             The update may have been applied but event parsing failed.",
            beacon_address
        );
        Ok(tx_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::sol_types::SolValue;

    #[test]
    fn test_abi_encode_inputs() {
        let measurement = U256::from(1000000000000000000u128);
        let nonce = U256::from(1704067200u64); // Example timestamp

        let inputs = (measurement, nonce).abi_encode();

        // ABI-encoded (uint256, uint256) should be 64 bytes
        assert_eq!(inputs.len(), 64);
    }

    #[test]
    fn test_parse_measurement() {
        // Test decimal string parsing
        let result = U256::from_str("1000000000000000000");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), U256::from(1000000000000000000u128));
    }
}
