use alloy::primitives::Address;
use std::str::FromStr;

use crate::models::{
    AppState, BatchCreatePerpcityBeaconResponse, BatchUpdateBeaconResponse, BeaconUpdateData,
    BeaconUpdateResult,
};
use crate::routes::{IBeacon, IBeaconFactory, IBeaconRegistry, IMulticall3};
use crate::services::transaction::events::parse_beacon_created_events_from_multicall;
use crate::services::transaction::execution::execute_transaction_serialized;

/// Execute batch creation of Perpcity beacons with multicall3
///
/// This function handles the complete business logic for batch beacon creation,
/// including validation, multicall execution, and result processing.
///
/// # Arguments
/// * `state` - Application state
/// * `count` - Number of beacons to create (1-100)
/// * `owner_address` - Address that will own the created beacons
///
/// # Returns
/// BatchCreatePerpcityBeaconResponse with results
pub async fn batch_create_perpcity_beacon(
    state: &AppState,
    count: u32,
    owner_address: Address,
) -> Result<BatchCreatePerpcityBeaconResponse, String> {
    tracing::info!("Starting batch creation of {} Perpcity beacons", count);

    // Validate the count
    if count == 0 || count > 100 {
        return Err(format!("Invalid beacon count: {count}"));
    }

    // Process all beacon creations in a single serialized transaction using multicall for efficiency
    let batch_results = execute_transaction_serialized(async {
        // Check if we have a multicall3 contract address configured
        if let Some(multicall_address) = state.multicall3_address {
            // Use multicall3 for atomic batch beacon creation
            batch_create_beacons_with_multicall3(state, multicall_address, count, owner_address)
                .await
        } else {
            // No multicall3 configured - return error for all beacon creations
            let error_msg =
                "Batch operations require Multicall3 contract address to be configured".to_string();
            tracing::error!("{}", error_msg);
            (1..=count).map(|i| (i, Err(error_msg.clone()))).collect()
        }
    })
    .await;

    // Process the results
    let mut beacon_addresses = Vec::new();
    let mut errors = Vec::new();

    for (_i, result) in batch_results {
        match result {
            Ok(address) => {
                beacon_addresses.push(address);
            }
            Err(error) => {
                errors.push(error);
            }
        }
    }

    let created_count = beacon_addresses.len() as u32;
    let failed_count = count - created_count;

    Ok(BatchCreatePerpcityBeaconResponse {
        created_count,
        beacon_addresses: beacon_addresses.clone(),
        failed_count,
        errors,
    })
}

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

    // Process all updates using multicall for efficient batching
    let batch_results = execute_transaction_serialized(async {
        // Check if we have a multicall3 contract address configured
        if let Some(multicall_address) = state.multicall3_address {
            // Use multicall3 for efficient batch execution - single transaction with multiple calls
            batch_update_with_multicall3(state, multicall_address, updates).await
        } else {
            // No multicall3 configured - return error for all updates
            let error_msg =
                "Batch operations require Multicall3 contract address to be configured".to_string();
            tracing::error!("{}", error_msg);
            updates
                .iter()
                .map(|update| (update.beacon_address.clone(), Err(error_msg.clone())))
                .collect()
        }
    })
    .await;

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

        // proof and public_signals are already Bytes (from 0x-hex JSON)
        let proof_bytes = update_data.proof.clone();
        let public_signals_bytes = update_data.public_signals.clone();

        // Create the updateData call data using the IBeacon interface
        let beacon_contract = IBeacon::new(beacon_address, &*state.provider);
        let call_data = beacon_contract
            .updateData(proof_bytes, public_signals_bytes)
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
    let multicall_contract = IMulticall3::new(multicall_address, &*state.provider);

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

                    // Transaction succeeded, now check individual call results
                    let mut results = Vec::new();

                    match multicall_contract.aggregate3(calls).call().await {
                        Ok(call_results) => {
                            // Iterate results in the same order as beacon_addresses
                            for (i, beacon_address) in beacon_addresses.iter().enumerate() {
                                if let Some(call_result) = call_results.get(i) {
                                    if call_result.success {
                                        results.push((beacon_address.clone(), Ok(tx_hash.clone())));
                                    } else {
                                        // Decode revert/return data
                                        let error_msg = if call_result.returnData.is_empty() {
                                            "Call failed with no return data".to_string()
                                        } else {
                                            format!(
                                                "Call failed: 0x{}",
                                                hex::encode(&call_result.returnData)
                                            )
                                        };
                                        results.push((beacon_address.clone(), Err(error_msg)));
                                    }
                                } else {
                                    results.push((
                                        beacon_address.clone(),
                                        Err("Missing result data for call".to_string()),
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            // Can't decode individual results - check overall transaction status
                            if receipt.status() {
                                // Transaction succeeded but we can't decode individual results
                                // Return partial success with warning
                                let warning = format!(
                                    "Batch update transaction succeeded but failed to decode individual results: {e}. Transaction hash: {tx_hash}"
                                );
                                tracing::warn!("{}", warning);
                                sentry::capture_message(&warning, sentry::Level::Warning);

                                // Return Ok for all beacons since transaction succeeded
                                // Include a warning that we couldn't verify individual success
                                for beacon_address in &beacon_addresses {
                                    results.push((beacon_address.clone(), Ok(tx_hash.clone())));
                                }

                                // Add one error entry with the warning so it appears in response
                                results.push((
                                    String::new(),
                                    Err(format!(
                                        "Warning: Could not decode individual results: {e}"
                                    )),
                                ));
                            } else {
                                // Transaction failed
                                let error_msg = format!(
                                    "Batch update transaction failed (status: false). Transaction hash: {tx_hash}"
                                );
                                tracing::error!("{}", error_msg);
                                sentry::capture_message(&error_msg, sentry::Level::Error);

                                // Return error for all beacons
                                for beacon_address in beacon_addresses {
                                    results.push((
                                        beacon_address,
                                        Err(format!("Transaction reverted: {tx_hash}")),
                                    ));
                                }
                            }
                        }
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

/// Execute batch beacon creation using multicall3 - single transaction with multiple calls
async fn batch_create_beacons_with_multicall3(
    state: &AppState,
    multicall_address: Address,
    count: u32,
    owner_address: Address,
) -> Vec<(u32, Result<String, String>)> {
    tracing::info!("Using Multicall3 for batch creation of {} beacons", count);

    // Prepare multicall3 calls - each beacon creation becomes a call in the multicall
    let mut calls = Vec::new();

    for _i in 1..=count {
        // Create the createBeacon call data using the IBeaconFactory interface
        let factory_contract = IBeaconFactory::new(state.beacon_factory_address, &*state.provider);
        let call_data = factory_contract
            .createBeacon(owner_address)
            .calldata()
            .clone();

        // Create multicall3 call
        let call = IMulticall3::Call3 {
            target: state.beacon_factory_address,
            allowFailure: false, // Atomic: all beacon creations must succeed or entire batch fails
            callData: call_data,
        };

        calls.push(call);
    }

    // Execute the multicall3 transaction - single transaction containing all beacon creations
    let multicall_contract = IMulticall3::new(multicall_address, &*state.provider);

    match multicall_contract.aggregate3(calls).send().await {
        Ok(pending_tx) => {
            tracing::info!("Multicall3 beacon creation transaction sent, waiting for receipt...");
            match pending_tx.get_receipt().await {
                Ok(receipt) => {
                    tracing::info!(
                        "Multicall3 beacon creation confirmed: {:?}",
                        receipt.transaction_hash
                    );

                    // Parse beacon addresses from the event logs
                    let beacon_addresses = parse_beacon_created_events_from_multicall(
                        &receipt,
                        state.beacon_factory_address,
                        count,
                    );

                    match beacon_addresses {
                        Ok(addresses) => {
                            // Register all created beacons with the registry in another multicall
                            match register_beacons_with_multicall3(
                                state,
                                multicall_address,
                                &addresses,
                            )
                            .await
                            {
                                Ok(_) => {
                                    // All succeeded
                                    addresses
                                        .into_iter()
                                        .enumerate()
                                        .map(|(i, addr)| ((i + 1) as u32, Ok(addr)))
                                        .collect()
                                }
                                Err(e) => {
                                    // Beacons were created successfully but registration failed
                                    // Return partial success: beacons exist on-chain but aren't registered
                                    let warning_msg = format!(
                                        "Beacons created but registration failed: {e}. Note: registerBeacon is idempotent and can be retried"
                                    );
                                    tracing::warn!("{}", warning_msg);
                                    sentry::capture_message(&warning_msg, sentry::Level::Warning);

                                    // Return Ok for each created beacon address
                                    // The warning will appear in the errors list during result processing
                                    let mut results: Vec<(u32, Result<String, String>)> = addresses
                                        .into_iter()
                                        .enumerate()
                                        .map(|(i, addr)| ((i + 1) as u32, Ok(addr)))
                                        .collect();

                                    // Add a single warning entry to inform about registration failure
                                    results.push((0, Err(warning_msg)));
                                    results
                                }
                            }
                        }
                        Err(e) => {
                            let error_msg =
                                format!("Failed to parse beacon addresses from multicall: {e}");
                            tracing::error!("{}", error_msg);
                            (1..=count).map(|i| (i, Err(error_msg.clone()))).collect()
                        }
                    }
                }
                Err(e) => {
                    let error_msg =
                        format!("Failed to get multicall3 beacon creation receipt: {e}");
                    tracing::error!("{}", error_msg);
                    (1..=count).map(|i| (i, Err(error_msg.clone()))).collect()
                }
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to send multicall3 beacon creation transaction: {e}");
            tracing::error!("{}", error_msg);
            (1..=count).map(|i| (i, Err(error_msg.clone()))).collect()
        }
    }
}

/// Register multiple beacons using multicall3
///
/// This function is idempotent - calling registerBeacon multiple times on the same
/// beacon is safe. The contract just sets `beacons[beacon] = true` and re-emits the event.
/// This means registration can be safely retried if it fails.
async fn register_beacons_with_multicall3(
    state: &AppState,
    multicall_address: Address,
    beacon_addresses: &[String],
) -> Result<(), String> {
    tracing::info!(
        "Using Multicall3 for batch registration of {} beacons",
        beacon_addresses.len()
    );

    let mut calls = Vec::new();

    for beacon_addr_str in beacon_addresses {
        let beacon_address = Address::from_str(beacon_addr_str)
            .map_err(|e| format!("Invalid beacon address {beacon_addr_str}: {e}"))?;

        // Create the registerBeacon call data using the IBeaconRegistry interface
        let registry_contract =
            IBeaconRegistry::new(state.perpcity_registry_address, &*state.provider);
        let call_data = registry_contract
            .registerBeacon(beacon_address)
            .calldata()
            .clone();

        // Create multicall3 call
        let call = IMulticall3::Call3 {
            target: state.perpcity_registry_address,
            allowFailure: false, // Atomic: all registrations must succeed
            callData: call_data,
        };

        calls.push(call);
    }

    // Execute the multicall3 registration transaction
    let multicall_contract = IMulticall3::new(multicall_address, &*state.provider);

    match multicall_contract.aggregate3(calls).send().await {
        Ok(pending_tx) => match pending_tx.get_receipt().await {
            Ok(receipt) => {
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
