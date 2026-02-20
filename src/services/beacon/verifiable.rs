use alloy::primitives::{Address, U256};
use std::time::Duration;
use tokio::time::timeout;
use tracing;

use crate::models::AppState;
use crate::routes::IDichotomousBeaconFactory;
use crate::services::transaction::events::parse_beacon_created_event;
use crate::services::transaction::execution::is_nonce_error;

/// Creates a verifiable beacon using a DichotomousBeaconFactory at the given address.
///
/// This is the core function that takes all parameters explicitly, including
/// the factory address. Used by the unified beacon creation dispatch.
pub async fn create_verifiable_beacon_with_factory(
    state: &AppState,
    factory_address: Address,
    verifier_address: Address,
    initial_data: u128,
    initial_cardinality: u32,
) -> Result<Address, String> {
    tracing::info!("Creating verifiable beacon with:");
    tracing::info!("  Factory: {}", factory_address);
    tracing::info!("  Verifier: {}", verifier_address);
    tracing::info!("  Initial data: {}", initial_data);
    tracing::info!("  Initial cardinality: {}", initial_cardinality);

    // Acquire a wallet from the pool
    let wallet_handle = state
        .wallet_manager
        .acquire_any_wallet()
        .await
        .map_err(|e| format!("Failed to acquire wallet: {e}"))?;

    let wallet_address = wallet_handle.address();
    tracing::info!(
        "Acquired wallet {} for verifiable beacon creation",
        wallet_address
    );

    // Build provider with the acquired wallet
    let provider = wallet_handle
        .build_provider(&state.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;

    // Create contract instance using the wallet's provider
    let contract = IDichotomousBeaconFactory::new(factory_address, &provider);

    // Send beacon creation transaction
    tracing::info!("Creating verifiable beacon with wallet {}", wallet_address);
    let pending_tx = match contract
        .createBeacon(
            verifier_address,
            U256::from(initial_data),
            initial_cardinality,
        )
        .send()
        .await
    {
        Ok(pending) => Ok(pending),
        Err(e) => {
            let error_msg = format!("Failed to send createBeacon transaction: {e}");
            tracing::error!("{}", error_msg);

            if is_nonce_error(&error_msg) {
                tracing::warn!("Nonce error detected, transaction failed");
            }

            Err(error_msg)
        }
    }
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

    let tx_hash = receipt.transaction_hash;
    tracing::info!(
        "Verifiable beacon creation transaction confirmed with hash: {:?}",
        tx_hash
    );

    // Check transaction status
    if !receipt.status() {
        let error_msg =
            format!("Verifiable beacon creation transaction {tx_hash} reverted (status: false)");
        tracing::error!("{}", error_msg);
        sentry::capture_message(
            &format!(
                "Verifiable beacon creation transaction reverted: {tx_hash} (factory: {factory_address})"
            ),
            sentry::Level::Error,
        );
        return Err(error_msg);
    }

    tracing::info!("Verifiable beacon creation transaction succeeded (status: true)");

    // Parse the BeaconCreated event from the transaction receipt
    let beacon_address = parse_beacon_created_event(&receipt, factory_address).map_err(|e| {
        tracing::error!("Failed to parse BeaconCreated event: {}", e);
        sentry::capture_message(
            &format!("Failed to parse verifiable BeaconCreated event: {e}"),
            sentry::Level::Error,
        );
        e
    })?;

    tracing::info!(
        "Verifiable beacon created successfully - Beacon: {}",
        beacon_address
    );

    Ok(beacon_address)
}
