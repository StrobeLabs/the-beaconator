use alloy::primitives::Address;
use std::str::FromStr;

use crate::AlloyProvider;
use crate::models::{AppState, BatchUpdateBeaconResponse, BeaconUpdateData, BeaconUpdateResult};
use crate::routes::{IBeacon, IBeaconRegistry, IMulticall3};

/// Execute batch updates of beacon data with multicall3
///
/// This function handles the complete business logic for batch beacon updates,
/// including validation, multicall execution, and result processing.
///
/// # Arguments
/// * `state` - Application state
/// * `updates` - Vector of beacon update data
///
/// # Returns
/// BatchUpdateBeaconResponse with results
pub async fn batch_update_beacon(
    state: &AppState,
    updates: &[BeaconUpdateData],
) -> Result<BatchUpdateBeaconResponse, String> {
    tracing::info!("Starting batch update of {} beacons", updates.len());

    // Validate request
    if updates.is_empty() {
        return Err("Batch update request with no updates".to_string());
    }

    if updates.len() > 100 {
        return Err("Batch update request exceeds maximum of 100 updates".to_string());
    }

    // Group updates by owner wallet to ensure correct wallet is used for each beacon
    let mut updates_by_wallet: std::collections::HashMap<Address, Vec<&BeaconUpdateData>> =
        std::collections::HashMap::new();
    let mut parse_errors: Vec<(String, String)> = Vec::new();

    for update in updates {
        // Parse beacon address
        match Address::from_str(&update.beacon_address) {
            Ok(beacon_addr) => {
                // Get the wallet that owns this beacon (or any available wallet if no owner set)
                match state.wallet_manager.acquire_for_beacon(&beacon_addr).await {
                    Ok(handle) => {
                        let wallet_addr = handle.address();
                        // Release the handle immediately - we just need to know which wallet to use
                        drop(handle);
                        updates_by_wallet
                            .entry(wallet_addr)
                            .or_default()
                            .push(update);
                    }
                    Err(e) => {
                        parse_errors.push((
                            update.beacon_address.clone(),
                            format!("Failed to determine wallet for beacon: {e}"),
                        ));
                    }
                }
            }
            Err(e) => {
                parse_errors.push((
                    update.beacon_address.clone(),
                    format!("Invalid beacon address: {e}"),
                ));
            }
        }
    }

    // Process each wallet's updates separately
    let mut batch_results: Vec<(String, Result<String, String>)> = Vec::new();

    // Add parse errors to results
    for (beacon_addr, error) in parse_errors {
        batch_results.push((beacon_addr, Err(error)));
    }

    // Process updates for each wallet
    for (wallet_addr, wallet_updates) in updates_by_wallet {
        // Acquire the specific wallet for this batch
        let wallet_handle = match state
            .wallet_manager
            .acquire_specific_wallet(&wallet_addr)
            .await
        {
            Ok(handle) => handle,
            Err(e) => {
                // Mark all updates for this wallet as failed
                let error_msg = format!("Failed to acquire wallet {wallet_addr}: {e}");
                tracing::error!("{}", error_msg);
                for update in wallet_updates {
                    batch_results.push((update.beacon_address.clone(), Err(error_msg.clone())));
                }
                continue;
            }
        };

        tracing::info!(
            "Acquired wallet {} for batch update of {} beacons",
            wallet_addr,
            wallet_updates.len()
        );

        // Build provider with the acquired wallet
        let provider = match wallet_handle.build_provider(&state.rpc_url) {
            Ok(p) => p,
            Err(e) => {
                let error_msg = format!("Failed to build provider for wallet {wallet_addr}: {e}");
                tracing::error!("{}", error_msg);
                for update in wallet_updates {
                    batch_results.push((update.beacon_address.clone(), Err(error_msg.clone())));
                }
                continue;
            }
        };

        // Process this wallet's updates using multicall
        if let Some(multicall_address) = state.multicall3_address {
            // Convert &[&BeaconUpdateData] to &[BeaconUpdateData] for the function call
            let updates_slice: Vec<BeaconUpdateData> =
                wallet_updates.iter().map(|u| (*u).clone()).collect();
            let wallet_batch_results =
                batch_update_with_multicall3(state, &provider, multicall_address, &updates_slice)
                    .await;
            batch_results.extend(wallet_batch_results);
        } else {
            let error_msg =
                "Batch operations require Multicall3 contract address to be configured".to_string();
            tracing::error!("{}", error_msg);
            for update in wallet_updates {
                batch_results.push((update.beacon_address.clone(), Err(error_msg.clone())));
            }
        }
    }

    // Process the results
    let mut results = Vec::new();
    let mut successful_updates = 0;
    let mut failed_updates = 0;

    for (beacon_address, result) in batch_results {
        match result {
            Ok(tx_hash) => {
                successful_updates += 1;
                results.push(BeaconUpdateResult {
                    beacon_address: beacon_address.clone(),
                    success: true,
                    transaction_hash: Some(tx_hash.clone()),
                    error: None,
                });
                tracing::info!(
                    "Successfully updated beacon {} with tx hash: {}",
                    beacon_address,
                    tx_hash
                );
            }
            Err(error) => {
                failed_updates += 1;
                results.push(BeaconUpdateResult {
                    beacon_address: beacon_address.clone(),
                    success: false,
                    transaction_hash: None,
                    error: Some(error.clone()),
                });
                tracing::error!("Failed to update beacon {}: {}", beacon_address, error);
            }
        }
    }

    Ok(BatchUpdateBeaconResponse {
        results,
        total_requested: updates.len(),
        successful_updates,
        failed_updates,
    })
}

/// Execute batch updates using multicall3 - single transaction with multiple calls
async fn batch_update_with_multicall3(
    state: &AppState,
    provider: &AlloyProvider,
    multicall_address: Address,
    updates: &[BeaconUpdateData],
) -> Vec<(String, Result<String, String>)> {
    tracing::info!(
        "Using Multicall3 for batch update of {} beacons",
        updates.len()
    );

    // Prepare multicall3 calls - each beacon update becomes a call in the multicall
    let mut calls = Vec::new();
    let mut beacon_addresses = Vec::new();
    let mut invalid_addresses = Vec::new();

    for update_data in updates {
        // Parse beacon address
        let beacon_address = match Address::from_str(&update_data.beacon_address) {
            Ok(addr) => addr,
            Err(e) => {
                // Track invalid address for error reporting
                invalid_addresses.push((
                    update_data.beacon_address.clone(),
                    format!("Invalid beacon address: {e}"),
                ));
                continue; // Skip this update but continue processing others
            }
        };

        // proof and inputs are already Bytes (from 0x-hex JSON)
        let proof_bytes = update_data.proof.clone();
        let inputs_bytes = update_data.public_signals.clone();

        // Create the update call data using the IBeacon interface (read provider for calldata generation)
        let beacon_contract = IBeacon::new(beacon_address, &*state.read_provider);
        let call_data = beacon_contract
            .update(proof_bytes, inputs_bytes)
            .calldata()
            .clone();

        // Create multicall3 call - allow individual failures to not revert entire batch
        let call = IMulticall3::Call3 {
            target: beacon_address,
            allowFailure: true, // Allow individual beacon updates to fail without reverting entire batch
            callData: call_data,
        };

        calls.push(call);
        beacon_addresses.push(update_data.beacon_address.clone());
    }

    // Execute the multicall3 transaction - single transaction containing all beacon updates
    let multicall_contract = IMulticall3::new(multicall_address, provider);

    // First send the transaction
    match multicall_contract.aggregate3(calls.clone()).send().await {
        Ok(pending_tx) => {
            tracing::info!("Multicall3 batch update transaction sent, waiting for receipt...");
            match pending_tx.get_receipt().await {
                Ok(receipt) => {
                    tracing::info!(
                        "Multicall3 batch update confirmed: {:?}",
                        receipt.transaction_hash
                    );

                    let tx_hash = format!("{:?}", receipt.transaction_hash);

                    // First check transaction status
                    if !receipt.status() {
                        let error_msg = format!(
                            "Batch update transaction reverted (status: false). Transaction hash: {tx_hash}"
                        );
                        tracing::error!("{}", error_msg);
                        sentry::capture_message(&error_msg, sentry::Level::Error);

                        // Return error for all beacons
                        let mut results = Vec::new();
                        for beacon_address in beacon_addresses {
                            results.push((
                                beacon_address,
                                Err(format!("Transaction reverted: {tx_hash}")),
                            ));
                        }
                        for (beacon_address, original_error) in invalid_addresses {
                            results.push((beacon_address, Err(original_error)));
                        }
                        return results;
                    }

                    // Transaction succeeded - return Ok for all beacons
                    // With allowFailure: true, individual calls may have failed but
                    // the overall transaction succeeded. We report success based on
                    // the transaction confirmation rather than attempting to replay
                    // the call (which would simulate against current state, not the
                    // state at execution time).
                    let mut results = Vec::new();
                    for beacon_address in &beacon_addresses {
                        results.push((beacon_address.clone(), Ok(tx_hash.clone())));
                    }

                    // Add results for invalid addresses
                    for (beacon_address, error) in invalid_addresses {
                        results.push((beacon_address, Err(error)));
                    }

                    results
                }
                Err(e) => {
                    let error_msg = format!("Failed to get multicall3 batch update receipt: {e}");
                    tracing::error!("{}", error_msg);

                    // Return errors for all attempted updates
                    let mut results = Vec::new();
                    for beacon_address in beacon_addresses {
                        results.push((beacon_address, Err(error_msg.clone())));
                    }
                    for (beacon_address, original_error) in invalid_addresses {
                        results.push((beacon_address, Err(original_error)));
                    }
                    results
                }
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to send multicall3 batch update transaction: {e}");
            tracing::error!("{}", error_msg);

            // Return errors for all attempted updates
            let mut results = Vec::new();
            for beacon_address in beacon_addresses {
                results.push((beacon_address, Err(error_msg.clone())));
            }
            for (beacon_address, error) in invalid_addresses {
                results.push((beacon_address, Err(error)));
            }
            results
        }
    }
}

/// Register multiple beacons using multicall3
///
/// This function is idempotent - calling registerBeacon multiple times on the same
/// beacon is safe.
pub async fn register_beacons_with_multicall3(
    state: &AppState,
    provider: &AlloyProvider,
    multicall_address: Address,
    registry_address: Address,
    beacon_addresses: &[String],
) -> Result<(), String> {
    tracing::info!(
        "Using Multicall3 for batch registration of {} beacons with registry {}",
        beacon_addresses.len(),
        registry_address
    );

    let mut calls = Vec::new();
    let registry_contract = IBeaconRegistry::new(registry_address, &*state.read_provider);

    for beacon_addr_str in beacon_addresses {
        let beacon_address = Address::from_str(beacon_addr_str)
            .map_err(|e| format!("Invalid beacon address {beacon_addr_str}: {e}"))?;

        let call_data = registry_contract
            .registerBeacon(beacon_address)
            .calldata()
            .clone();

        let call = IMulticall3::Call3 {
            target: registry_address,
            allowFailure: false,
            callData: call_data,
        };

        calls.push(call);
    }

    // Execute the multicall3 registration transaction
    let multicall_contract = IMulticall3::new(multicall_address, provider);

    match multicall_contract.aggregate3(calls).send().await {
        Ok(pending_tx) => match pending_tx.get_receipt().await {
            Ok(receipt) => {
                if !receipt.status() {
                    let tx_hash = format!("{:?}", receipt.transaction_hash);
                    return Err(format!(
                        "Beacon registration transaction reverted (tx: {tx_hash})"
                    ));
                }
                tracing::info!(
                    "Multicall3 beacon registration confirmed: {:?}",
                    receipt.transaction_hash
                );
                Ok(())
            }
            Err(e) => Err(format!("Failed to get registration receipt: {e}")),
        },
        Err(e) => Err(format!("Failed to send registration transaction: {e}")),
    }
}
