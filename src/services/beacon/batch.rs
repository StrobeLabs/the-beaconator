use alloy::primitives::Address;
use std::str::FromStr;

use crate::AlloyProvider;
use crate::models::beacon_type::BeaconTypeConfig;
use crate::models::{
    AppState, BatchCreateBeaconResponse, BatchUpdateBeaconResponse, BeaconUpdateData,
    BeaconUpdateResult,
};
use crate::routes::{IBeacon, IBeaconFactory, IBeaconRegistry, IMulticall3};
use crate::services::transaction::events::parse_beacon_created_events_from_multicall;

/// Execute batch creation of beacons using a BeaconTypeConfig.
///
/// Currently only supports Simple factory types via multicall3.
/// The beacon owner will be set to the acquired wallet's address.
pub async fn batch_create_beacons(
    state: &AppState,
    config: &BeaconTypeConfig,
    count: u32,
) -> Result<BatchCreateBeaconResponse, String> {
    tracing::info!(
        "Starting batch creation of {} '{}' beacons",
        count,
        config.slug
    );

    if count == 0 || count > 100 {
        return Err(format!("Invalid beacon count: {count}"));
    }

    // Acquire a wallet from the pool for all batch operations
    let wallet_handle = state
        .wallet_manager
        .acquire_any_wallet()
        .await
        .map_err(|e| format!("Failed to acquire wallet for batch creation: {e}"))?;

    let wallet_address = wallet_handle.address();
    let owner_address = wallet_address;
    tracing::info!(
        "Acquired wallet {} for batch beacon creation (owner: {})",
        wallet_address,
        owner_address
    );

    let provider = wallet_handle
        .build_provider(&state.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;

    let batch_results = if let Some(multicall_address) = state.multicall3_address {
        batch_create_beacons_with_multicall3(
            state,
            &provider,
            multicall_address,
            config.factory_address,
            count,
            owner_address,
        )
        .await
    } else {
        let error_msg =
            "Batch operations require Multicall3 contract address to be configured".to_string();
        tracing::error!("{}", error_msg);
        (1..=count).map(|i| (i, Err(error_msg.clone()))).collect()
    };

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

    // Register beacons if registry_address is configured
    if let Some(registry_address) = config.registry_address {
        if let Some(multicall3_address) = state.multicall3_address {
            if !beacon_addresses.is_empty() {
                tracing::info!(
                    "Registering {} beacons with registry {}",
                    beacon_addresses.len(),
                    registry_address
                );
                match register_beacons_with_multicall3(
                    state,
                    &provider,
                    multicall3_address,
                    registry_address,
                    &beacon_addresses,
                )
                .await
                {
                    Ok(_) => {
                        tracing::info!("Batch beacon registration completed");
                    }
                    Err(e) => {
                        tracing::warn!("Batch beacon registration failed: {}", e);
                    }
                }
            }
        } else if !beacon_addresses.is_empty() {
            tracing::warn!(
                "Registry {} is configured but MULTICALL3_ADDRESS is not set - skipping registration of {} beacons",
                registry_address,
                beacon_addresses.len()
            );
        }
    }

    Ok(BatchCreateBeaconResponse {
        beacon_type: config.slug.clone(),
        created_count,
        beacon_addresses,
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

        // proof and public_signals are already Bytes (from 0x-hex JSON)
        let proof_bytes = update_data.proof.clone();
        let public_signals_bytes = update_data.public_signals.clone();

        // Create the updateData call data using the IBeacon interface (read provider for calldata generation)
        let beacon_contract = IBeacon::new(beacon_address, &*state.read_provider);
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
    provider: &AlloyProvider,
    multicall_address: Address,
    factory_address: Address,
    count: u32,
    owner_address: Address,
) -> Vec<(u32, Result<String, String>)> {
    tracing::info!("Using Multicall3 for batch creation of {} beacons", count);

    let mut calls = Vec::new();
    let factory_contract = IBeaconFactory::new(factory_address, &*state.read_provider);

    for _i in 1..=count {
        let call_data = factory_contract
            .createBeacon(owner_address)
            .calldata()
            .clone();

        let call = IMulticall3::Call3 {
            target: factory_address,
            allowFailure: false,
            callData: call_data,
        };

        calls.push(call);
    }

    // Execute the multicall3 transaction - single transaction containing all beacon creations
    let multicall_contract = IMulticall3::new(multicall_address, provider);

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
                        factory_address,
                        count,
                    );

                    match beacon_addresses {
                        Ok(addresses) => {
                            // Return created beacon addresses; registration is handled by the caller
                            addresses
                                .into_iter()
                                .enumerate()
                                .map(|(i, addr)| ((i + 1) as u32, Ok(addr)))
                                .collect()
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
