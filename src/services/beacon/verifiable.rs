use alloy::primitives::{Address, U256};
use std::str::FromStr;
use std::time::Duration;
use tokio::time::timeout;
use tracing;

use crate::models::{AppState, CreateVerifiableBeaconRequest};
use crate::routes::{
    IDichotomousBeaconFactory, execute_transaction_serialized, get_fresh_nonce_from_alternate,
    is_nonce_error,
};

/// Creates a verifiable beacon using the DichotomousBeaconFactory.
///
/// Creates a new verifiable beacon with the specified verifier address,
/// initial data value, and TWAP observation cardinality.
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
