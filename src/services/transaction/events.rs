use alloy::primitives::{Address, FixedBytes, U256};
use tracing;

use crate::routes::{IBeacon, IPerpManager};

/// Parse the IndexUpdated event from transaction receipt
///
/// # Arguments
/// * `receipt` - The transaction receipt containing event logs
/// * `beacon_address` - The address of the beacon contract
///
/// # Returns
/// * `Ok(U256)` - The new index value from the IndexUpdated event
/// * `Err(String)` - Error message if event not found or parsing failed
pub fn parse_index_updated_event(
    receipt: &alloy::rpc::types::TransactionReceipt,
    beacon_address: Address,
) -> Result<U256, String> {
    // Look for the IndexUpdated event in the logs
    for log in receipt.logs().iter() {
        // Check if this log is from our beacon contract
        if log.address() == beacon_address {
            // Try to decode as IndexUpdated event
            match log.log_decode::<IBeacon::IndexUpdated>() {
                Ok(decoded_log) => {
                    let index = decoded_log.inner.data.index;
                    tracing::info!(
                        "Successfully parsed IndexUpdated event - new index: {}",
                        index
                    );
                    return Ok(index);
                }
                Err(_) => {
                    // Log is from beacon but not IndexUpdated event, continue
                }
            }
        }
    }

    let error_msg = "IndexUpdated event not found in transaction receipt";
    tracing::error!("{}", error_msg);
    tracing::error!("Total logs in receipt: {}", receipt.logs().len());
    sentry::capture_message(error_msg, sentry::Level::Error);
    Err(error_msg.to_string())
}

/// Parse the PerpCreated event from transaction receipt to get perp ID
///
/// # Arguments
/// * `receipt` - The transaction receipt containing event logs
/// * `perp_manager_address` - The address of the perp hook contract
///
/// # Returns
/// * `Ok(FixedBytes<32>)` - The perp ID from the PerpCreated event
/// * `Err(String)` - Error message if event not found or parsing failed
pub fn parse_perp_created_event(
    receipt: &alloy::rpc::types::TransactionReceipt,
    perp_manager_address: Address,
) -> Result<FixedBytes<32>, String> {
    // Look for the PerpCreated event in the logs
    for log in receipt.logs() {
        // Check if this log is from our perp hook contract
        if log.address() == perp_manager_address {
            // Try to decode as PerpCreated event
            if let Ok(decoded_log) = log.log_decode::<IPerpManager::PerpCreated>() {
                let event_data = decoded_log.inner.data;
                tracing::info!(
                    "Successfully parsed PerpCreated event - perp ID: {}",
                    event_data.perpId
                );
                return Ok(event_data.perpId);
            }
        }
    }

    Err("PerpCreated event not found in transaction receipt".to_string())
}

/// Parse the MakerPositionOpened event from transaction receipt
///
/// # Arguments
/// * `receipt` - The transaction receipt containing event logs
/// * `perp_manager_address` - The address of the perp hook contract
/// * `expected_perp_id` - The perp ID to match in the event
///
/// # Returns
/// * `Ok(U256)` - The maker position ID from the MakerPositionOpened event
/// * `Err(String)` - Error message if event not found or parsing failed
pub fn parse_maker_position_opened_event(
    receipt: &alloy::rpc::types::TransactionReceipt,
    perp_manager_address: Address,
    expected_perp_id: FixedBytes<32>,
) -> Result<U256, String> {
    // Look for the PositionOpened event in the logs (PerpManager uses unified event for maker and taker)
    for log in receipt.logs() {
        // Check if this log is from our perp manager contract
        if log.address() == perp_manager_address {
            // Try to decode as PositionOpened event
            if let Ok(decoded_log) = log.log_decode::<IPerpManager::PositionOpened>() {
                let event_data = decoded_log.inner.data;

                // Verify this is the event for our perp ID and it's a maker position
                if event_data.perpId == expected_perp_id && event_data.isMaker {
                    return Ok(event_data.posId);
                }
            }
        }
    }

    Err("PositionOpened event (maker) not found in transaction receipt".to_string())
}

// Tests moved to tests/unit_tests/transaction_events_tests.rs
