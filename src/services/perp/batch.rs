use alloy::primitives::{Address, FixedBytes};
use std::str::FromStr;
use tracing;

use crate::models::{AppState, DepositLiquidityForPerpRequest};

/// STUB: Batch liquidity deposits using multicall3
///
/// This function is not yet implemented. It currently returns an error for all deposits.
/// Individual liquidity deposits via `core::deposit_liquidity_for_perp` still work.
///
/// TODO: Implement multicall3 batch execution following the pattern in
/// `services/beacon/batch.rs::batch_create_beacons_with_multicall3`
pub async fn batch_deposit_liquidity_with_multicall3(
    _state: &AppState,
    _multicall_address: Address,
    deposits: &[DepositLiquidityForPerpRequest],
) -> Vec<(String, Result<String, String>)> {
    tracing::info!(
        "Using Multicall3 for batch liquidity deposit of {} perps",
        deposits.len()
    );

    // Build results in the same order as the input
    let mut results = Vec::new();
    let mut valid_perp_ids = Vec::new();

    for (i, deposit_request) in deposits.iter().enumerate() {
        let index = i + 1;
        tracing::info!(
            "Preparing liquidity deposit {}/{} for perp {}",
            index,
            deposits.len(),
            deposit_request.perp_id
        );

        // Parse the perp ID (PoolId as bytes32)
        let _perp_id = match FixedBytes::<32>::from_str(&deposit_request.perp_id) {
            Ok(id) => id,
            Err(e) => {
                let error_msg = format!(
                    "Failed to parse perp ID {index} ({}): {e}",
                    deposit_request.perp_id
                );
                results.push((deposit_request.perp_id.clone(), Err(error_msg)));
                continue;
            }
        };

        // Parse the margin amount (USDC in 6 decimals)
        let _margin_amount = match deposit_request.margin_amount_usdc.parse::<u128>() {
            Ok(amount) => amount,
            Err(e) => {
                let error_msg = format!(
                    "Failed to parse margin amount {index} ({}): {e}",
                    deposit_request.margin_amount_usdc
                );
                results.push((deposit_request.perp_id.clone(), Err(error_msg)));
                continue;
            }
        };

        // All margin validations are performed by on-chain modules

        // Validation passed - track this deposit for multicall processing
        valid_perp_ids.push(deposit_request.perp_id.clone());
    }

    // If we have valid deposits, return error - multicall3 implementation needed
    if !valid_perp_ids.is_empty() {
        let error_msg =
            "Batch liquidity deposits via Multicall3 not yet implemented in service layer";
        tracing::error!("{}", error_msg);
        tracing::error!(
            "This functionality needs to be migrated from the original routes implementation"
        );
        for perp_id in valid_perp_ids {
            results.push((perp_id, Err(error_msg.to_string())));
        }
    }

    results
}

/// STUB: Batch perp deployment using multicall3
///
/// This function is not yet implemented. It currently returns an error for all deployments.
/// Individual perp deployments via `core::deploy_perp_for_beacon` still work.
///
/// TODO: Implement multicall3 batch execution following the pattern in
/// `services/beacon/batch.rs::batch_create_beacons_with_multicall3`
pub async fn batch_deploy_perps_with_multicall3(
    _state: &AppState,
    _multicall_address: Address,
    beacon_addresses: &[String],
) -> Vec<(String, Result<String, String>)> {
    tracing::info!(
        "Batch perp deployment via Multicall3 requested for {} beacons",
        beacon_addresses.len()
    );

    let error_msg = "Batch perp deployment via Multicall3 not yet implemented in service layer";
    tracing::error!("{}", error_msg);
    tracing::error!(
        "Use individual deploy_perp_for_beacon calls or implement multicall3 batch logic"
    );

    // Return error for all deployments
    beacon_addresses
        .iter()
        .map(|beacon_addr| (beacon_addr.clone(), Err(error_msg.to_string())))
        .collect()
}
