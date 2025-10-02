use alloy::primitives::{Address, Bytes, U256};
use std::str::FromStr;
use std::time::Duration;
use tokio::time::timeout;
use tracing;

use crate::models::{AppState, CreateVerifiableBeaconRequest, UpdateVerifiableBeaconRequest};
use crate::routes::{
    IDichotomousBeaconFactory, IStepBeacon, execute_transaction_serialized,
    get_fresh_nonce_from_alternate, is_nonce_error,
};

/// Create a verifiable beacon using the DichotomousBeaconFactory
pub async fn create_verifiable_beacon(
    state: &AppState,
    request: CreateVerifiableBeaconRequest,
) -> Result<String, String> {
    // Check if dichotomous factory is configured
    let factory_address = match state.dichotomous_beacon_factory_address {
        Some(addr) => addr,
        None => {
            tracing::error!("Dichotomous beacon factory address not configured");
            return Err("Verifiable beacon factory not configured".to_string());
        }
    };

    // Parse verifier address
    let verifier_address = match Address::from_str(&request.verifier_address) {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("Invalid verifier address: {}", e);
            return Err("Invalid verifier address".to_string());
        }
    };

    tracing::info!("Creating verifiable beacon with:");
    tracing::info!("  Factory: {}", factory_address);
    tracing::info!("  Verifier: {}", verifier_address);
    tracing::info!("  Initial data: {}", request.initial_data);
    tracing::info!("  Initial cardinality: {}", request.initial_cardinality);

    // Create contract instance
    let contract = IDichotomousBeaconFactory::new(factory_address, &*state.provider);

    // Send beacon creation transaction
    let pending_tx = execute_transaction_serialized(async {
        tracing::info!("Creating verifiable beacon with primary RPC");
        let result = contract
            .createBeacon(
                verifier_address,
                U256::from(request.initial_data),
                request.initial_cardinality,
            )
            .send()
            .await;

        match result {
            Ok(pending) => Ok(pending),
            Err(e) => {
                let error_msg = format!("Failed to send createBeacon transaction: {e}");
                tracing::error!("{}", error_msg);

                // Check if nonce error and sync if needed
                if is_nonce_error(&error_msg) {
                    tracing::warn!(
                        "Nonce error detected, attempting to sync nonce from alternate RPC"
                    );
                    if let Ok(fresh_nonce) = get_fresh_nonce_from_alternate(state).await {
                        tracing::info!("Retrying with fresh nonce: {}", fresh_nonce);
                    }
                }
                Err(error_msg)
            }
        }
    })
    .await
    .map_err(|e| {
        tracing::error!("Transaction execution failed: {}", e);
        sentry::capture_message(
            &format!("Verifiable beacon creation failed: {e}"),
            sentry::Level::Error,
        );
        e
    })?;

    // Get transaction receipt with timeout
    let receipt = match timeout(Duration::from_secs(120), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => receipt,
        Ok(Err(e)) => {
            let error_msg = format!("Failed to get transaction receipt: {e}");
            tracing::error!("{}", error_msg);
            sentry::capture_message(
                &format!("Failed to get verifiable beacon creation receipt: {e}"),
                sentry::Level::Error,
            );
            return Err(error_msg);
        }
        Err(_) => {
            let error_msg = "Timeout waiting for transaction receipt".to_string();
            tracing::error!("{}", error_msg);
            sentry::capture_message(
                "Timeout waiting for verifiable beacon creation receipt",
                sentry::Level::Error,
            );
            return Err(error_msg);
        }
    };

    // Parse the BeaconCreated event from the transaction receipt
    let beacon_address = {
        let mut beacon_addr = None;

        // Look for the BeaconCreated event in the logs
        for log in receipt.inner.logs().iter() {
            // Check if this log is from our factory contract
            if log.address() == factory_address {
                // Try to decode as BeaconCreated event
                match log.log_decode::<IDichotomousBeaconFactory::BeaconCreated>() {
                    Ok(decoded_log) => {
                        beacon_addr = Some(decoded_log.inner.data.beacon);
                        tracing::info!(
                            "Successfully parsed BeaconCreated event - beacon: {}, verifier: {}",
                            decoded_log.inner.data.beacon,
                            decoded_log.inner.data.verifier
                        );
                        break;
                    }
                    Err(e) => {
                        tracing::debug!("Could not decode log as BeaconCreated: {}", e);
                    }
                }
            }
        }

        beacon_addr.ok_or_else(|| {
            let error_msg = "BeaconCreated event not found in transaction receipt".to_string();
            tracing::error!("{}", error_msg);
            tracing::error!("Total logs in receipt: {}", receipt.inner.logs().len());
            sentry::capture_message("BeaconCreated event not found", sentry::Level::Error);
            error_msg
        })?
    };

    tracing::info!(
        "Verifiable beacon created successfully - Beacon: {}",
        beacon_address
    );
    sentry::capture_message(
        &format!("Verifiable beacon created: {beacon_address}"),
        sentry::Level::Info,
    );

    Ok(format!("Beacon address: {beacon_address}"))
}

/// Update a verifiable beacon with zero-knowledge proof
pub async fn update_verifiable_beacon(
    state: &AppState,
    request: UpdateVerifiableBeaconRequest,
) -> Result<String, String> {
    // Parse beacon address
    let beacon_address = match Address::from_str(&request.beacon_address) {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("Invalid beacon address: {}", e);
            return Err("Invalid beacon address".to_string());
        }
    };

    // Parse proof and public signals from hex strings
    let proof_bytes = match hex::decode(request.proof.trim_start_matches("0x")) {
        Ok(bytes) => Bytes::from(bytes),
        Err(e) => {
            tracing::error!("Invalid proof hex: {}", e);
            return Err("Invalid proof hex".to_string());
        }
    };

    let signals_bytes = match hex::decode(request.public_signals.trim_start_matches("0x")) {
        Ok(bytes) => Bytes::from(bytes),
        Err(e) => {
            tracing::error!("Invalid public signals hex: {}", e);
            return Err("Invalid public signals hex".to_string());
        }
    };

    tracing::info!(
        "Updating verifiable beacon {} with proof ({} bytes) and signals ({} bytes)",
        beacon_address,
        proof_bytes.len(),
        signals_bytes.len()
    );

    // Create contract instance
    let contract = IStepBeacon::new(beacon_address, &*state.provider);

    // Send update transaction
    let pending_tx_result = execute_transaction_serialized(async {
        tracing::info!("Updating verifiable beacon with primary RPC");
        let result = contract
            .updateData(proof_bytes.clone(), signals_bytes.clone())
            .send()
            .await;

        match result {
            Ok(pending) => Ok(pending),
            Err(e) => {
                let error_msg = format!("Failed to send updateData transaction: {e}");
                tracing::error!("{}", error_msg);

                // Check for specific errors
                if error_msg.contains("ProofAlreadyUsed") {
                    tracing::warn!("Proof has already been used");
                    sentry::capture_message("Proof reuse attempted", sentry::Level::Warning);
                } else if error_msg.contains("InvalidProof") {
                    tracing::warn!("Invalid proof provided");
                    sentry::capture_message("Invalid proof submitted", sentry::Level::Warning);
                } else if is_nonce_error(&error_msg) {
                    tracing::warn!("Nonce error detected, attempting to sync nonce");
                    if let Ok(fresh_nonce) = get_fresh_nonce_from_alternate(state).await {
                        tracing::info!("Retrying with fresh nonce: {}", fresh_nonce);
                    }
                }
                Err(error_msg)
            }
        }
    })
    .await;

    // Handle transaction execution result
    let pending_tx = match pending_tx_result {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!("Transaction execution failed: {}", e);

            // Return appropriate error based on the failure type
            if e.contains("ProofAlreadyUsed") {
                return Err("Proof has already been used".to_string());
            } else if e.contains("InvalidProof") {
                return Err("Invalid proof provided".to_string());
            } else {
                sentry::capture_message(&e, sentry::Level::Error);
                return Err("Failed to update verifiable beacon".to_string());
            }
        }
    };

    // Get transaction receipt with timeout
    let receipt = match timeout(Duration::from_secs(120), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => receipt,
        Ok(Err(e)) => {
            let error_msg = format!("Failed to get transaction receipt: {e}");
            tracing::error!("{}", error_msg);
            sentry::capture_message(
                &format!("Failed to get verifiable beacon update receipt: {e}"),
                sentry::Level::Error,
            );
            return Err(error_msg);
        }
        Err(_) => {
            let error_msg = "Timeout waiting for transaction receipt".to_string();
            tracing::error!("{}", error_msg);
            sentry::capture_message(
                "Timeout waiting for verifiable beacon update receipt",
                sentry::Level::Error,
            );
            return Err(error_msg);
        }
    };

    tracing::info!(
        "Verifiable beacon update confirmed in block {:?}",
        receipt.block_number
    );

    let message = "Verifiable beacon updated successfully";
    tracing::info!("{}", message);
    sentry::capture_message(
        &format!("Verifiable beacon {beacon_address} updated successfully"),
        sentry::Level::Info,
    );

    Ok(format!("Transaction hash: {:?}", receipt.transaction_hash))
}
