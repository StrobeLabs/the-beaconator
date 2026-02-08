/// Multicall3 Service
///
/// Provides utilities for batching multiple contract calls into a single transaction
/// using the Multicall3 contract pattern.
use alloy::primitives::{Address, Bytes};
use alloy::rpc::types::TransactionReceipt;
use tracing;

use crate::models::AppState;
use crate::routes::IMulticall3;

/// Execute multiple contract calls in a single transaction using Multicall3
///
/// # Arguments
/// * `state` - Application state containing provider and contract addresses
/// * `multicall_address` - Address of the Multicall3 contract
/// * `calls` - Vector of calls to execute (target address + calldata)
///
/// # Returns
/// Transaction receipt or error message
pub async fn execute_multicall(
    state: &AppState,
    multicall_address: Address,
    calls: Vec<IMulticall3::Call3>,
) -> Result<TransactionReceipt, String> {
    tracing::info!(
        "Executing multicall with {} calls to Multicall3 at {}",
        calls.len(),
        multicall_address
    );

    if calls.is_empty() {
        return Err("No calls provided for multicall".to_string());
    }

    // Acquire a wallet from the pool for the multicall transaction
    let wallet_handle = state
        .wallet_manager
        .acquire_any_wallet()
        .await
        .map_err(|e| format!("Failed to acquire wallet for multicall: {e}"))?;

    let wallet_address = wallet_handle.address();
    tracing::info!("Acquired wallet {} for multicall execution", wallet_address);

    // Build provider with the acquired wallet
    let provider = wallet_handle
        .build_provider(&state.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;

    // Create multicall contract instance
    let multicall_contract = IMulticall3::new(multicall_address, &provider);

    // Execute the multicall transaction
    let pending_tx = multicall_contract
        .aggregate3(calls.clone())
        .send()
        .await
        .map_err(|e| {
            let error_msg = format!("Failed to send multicall transaction: {e}");
            tracing::error!("{}", error_msg);
            sentry::capture_message(&error_msg, sentry::Level::Error);
            error_msg
        })?;

    tracing::info!("Multicall transaction sent, awaiting confirmation...");

    // Wait for transaction confirmation with timeout
    let receipt =
        tokio::time::timeout(std::time::Duration::from_secs(30), pending_tx.get_receipt())
            .await
            .map_err(|_| {
                let error_msg = "Timeout waiting for multicall transaction receipt";
                tracing::error!("{}", error_msg);
                sentry::capture_message(error_msg, sentry::Level::Warning);
                error_msg.to_string()
            })?
            .map_err(|e| {
                let error_msg = format!("Failed to get multicall transaction receipt: {e}");
                tracing::error!("{}", error_msg);
                sentry::capture_message(&error_msg, sentry::Level::Error);
                error_msg
            })?;

    let tx_hash = receipt.transaction_hash;
    tracing::info!("Multicall transaction confirmed: {:?}", tx_hash);

    // Check transaction status - only return success if status is true
    if receipt.status() {
        tracing::info!("Multicall transaction succeeded (status: true)");
        Ok(receipt)
    } else {
        let error_msg = format!("Multicall transaction {tx_hash} reverted (status: false)");
        tracing::error!("{}", error_msg);
        sentry::capture_message(&error_msg, sentry::Level::Error);
        Err(error_msg)
    }
}

/// Build a multicall Call3 struct from target address and encoded calldata
///
/// # Arguments
/// * `target` - Target contract address
/// * `calldata` - ABI-encoded function call data
/// * `allow_failure` - Whether to allow this call to fail without reverting the whole multicall
///
/// # Returns
/// IMulticall3::Call3 struct
pub fn build_multicall_call(
    target: Address,
    calldata: Bytes,
    allow_failure: bool,
) -> IMulticall3::Call3 {
    IMulticall3::Call3 {
        target,
        callData: calldata,
        allowFailure: allow_failure,
    }
}

/// Execute multiple beacon creation calls via multicall3
///
/// This is a specialized multicall for batch beacon creation operations.
///
/// # Arguments
/// * `state` - Application state
/// * `multicall_address` - Address of Multicall3 contract
/// * `beacon_calls` - Vector of beacon creation calls
///
/// # Returns
/// Transaction receipt or error
pub async fn execute_batch_beacon_creation_multicall(
    state: &AppState,
    multicall_address: Address,
    beacon_calls: Vec<IMulticall3::Call3>,
) -> Result<TransactionReceipt, String> {
    if beacon_calls.is_empty() {
        return Err("No beacon creation calls provided".to_string());
    }

    tracing::info!(
        "Executing batch beacon creation with {} calls via Multicall3",
        beacon_calls.len()
    );

    execute_multicall(state, multicall_address, beacon_calls).await
}

/// Execute multiple liquidity deposit calls via multicall3
///
/// This is a specialized multicall for batch liquidity deposit operations.
///
/// # Arguments
/// * `state` - Application state
/// * `multicall_address` - Address of Multicall3 contract
/// * `deposit_calls` - Vector of liquidity deposit calls
///
/// # Returns
/// Transaction receipt or error
pub async fn execute_batch_liquidity_deposit_multicall(
    state: &AppState,
    multicall_address: Address,
    deposit_calls: Vec<IMulticall3::Call3>,
) -> Result<TransactionReceipt, String> {
    if deposit_calls.is_empty() {
        return Err("No liquidity deposit calls provided".to_string());
    }

    tracing::info!(
        "Executing batch liquidity deposit with {} calls via Multicall3",
        deposit_calls.len()
    );

    execute_multicall(state, multicall_address, deposit_calls).await
}

/// Parse results from a multicall3 aggregate3 transaction
///
/// # Arguments
/// * `receipt` - Transaction receipt from multicall execution
/// * `expected_count` - Expected number of results
///
/// # Returns
/// Vector of (success: bool, returnData: Bytes) tuples
pub fn parse_multicall_results(
    receipt: &TransactionReceipt,
    expected_count: usize,
) -> Result<Vec<(bool, Bytes)>, String> {
    // Note: Actual parsing would decode the return values from the receipt logs
    // For now, this is a placeholder that returns expected structure

    tracing::debug!(
        "Parsing multicall results from transaction {:?}, expecting {} results",
        receipt.transaction_hash,
        expected_count
    );

    // In a real implementation, we would:
    // 1. Find the Multicall3.Aggregate3 event or decode return data
    // 2. Extract the (bool success, bytes returnData)[] array
    // 3. Validate against expected_count

    // Placeholder: return empty results
    Ok(vec![])
}

/// Validate that all multicall results were successful
///
/// # Arguments
/// * `results` - Results from parse_multicall_results
///
/// # Returns
/// Ok if all successful, Err with failure details otherwise
pub fn validate_multicall_success(results: &[(bool, Bytes)]) -> Result<(), String> {
    let failures: Vec<usize> = results
        .iter()
        .enumerate()
        .filter_map(|(i, (success, _))| if !success { Some(i) } else { None })
        .collect();

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Multicall had {} failures at indices: {:?}",
            failures.len(),
            failures
        ))
    }
}

// Tests moved to tests/unit_tests/transaction_multicall_tests.rs
