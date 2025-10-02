use alloy::primitives::{Address, FixedBytes, U256};
use tracing;

use crate::routes::{IBeacon, IBeaconFactory, IPerpHook};

/// Parse the BeaconCreated event from transaction receipt to get beacon address
///
/// # Arguments
/// * `receipt` - The transaction receipt containing event logs
/// * `factory_address` - The address of the beacon factory contract
///
/// # Returns
/// * `Ok(Address)` - The address of the newly created beacon
/// * `Err(String)` - Error message if event not found or parsing failed
pub fn parse_beacon_created_event(
    receipt: &alloy::rpc::types::TransactionReceipt,
    factory_address: Address,
) -> Result<Address, String> {
    // Look for the BeaconCreated event in the logs
    for log in receipt.logs().iter() {
        // Check if this log is from our factory contract
        if log.address() == factory_address {
            // Try to decode as BeaconCreated event
            match log.log_decode::<IBeaconFactory::BeaconCreated>() {
                Ok(decoded_log) => {
                    let beacon = decoded_log.inner.data.beacon;
                    tracing::info!(
                        "Successfully parsed BeaconCreated event - beacon address: {}",
                        beacon
                    );
                    return Ok(beacon);
                }
                Err(_) => {
                    // Log is from factory but not BeaconCreated event, continue
                }
            }
        }
    }

    let error_msg = "BeaconCreated event not found in transaction receipt";
    tracing::error!("{}", error_msg);
    tracing::error!("Total logs in receipt: {}", receipt.logs().len());
    sentry::capture_message(error_msg, sentry::Level::Error);
    Err(error_msg.to_string())
}

/// Parse the DataUpdated event from transaction receipt
///
/// # Arguments
/// * `receipt` - The transaction receipt containing event logs
/// * `beacon_address` - The address of the beacon contract
///
/// # Returns
/// * `Ok(U256)` - The new data value from the DataUpdated event
/// * `Err(String)` - Error message if event not found or parsing failed
pub fn parse_data_updated_event(
    receipt: &alloy::rpc::types::TransactionReceipt,
    beacon_address: Address,
) -> Result<U256, String> {
    // Look for the DataUpdated event in the logs
    for log in receipt.logs().iter() {
        // Check if this log is from our beacon contract
        if log.address() == beacon_address {
            // Try to decode as DataUpdated event
            match log.log_decode::<IBeacon::DataUpdated>() {
                Ok(decoded_log) => {
                    let data = decoded_log.inner.data.data;
                    tracing::info!("Successfully parsed DataUpdated event - new data: {}", data);
                    return Ok(data);
                }
                Err(_) => {
                    // Log is from beacon but not DataUpdated event, continue
                }
            }
        }
    }

    let error_msg = "DataUpdated event not found in transaction receipt";
    tracing::error!("{}", error_msg);
    tracing::error!("Total logs in receipt: {}", receipt.logs().len());
    sentry::capture_message(error_msg, sentry::Level::Error);
    Err(error_msg.to_string())
}

/// Parse multiple BeaconCreated events from a multicall transaction receipt
///
/// # Arguments
/// * `receipt` - The transaction receipt containing event logs
/// * `factory_address` - The address of the beacon factory contract
/// * `expected_count` - The expected number of BeaconCreated events
///
/// # Returns
/// * `Ok(Vec<String>)` - Vector of beacon addresses as strings
/// * `Err(String)` - Error message if expected count doesn't match or parsing failed
pub fn parse_beacon_created_events_from_multicall(
    receipt: &alloy::rpc::types::TransactionReceipt,
    factory_address: Address,
    expected_count: u32,
) -> Result<Vec<String>, String> {
    let mut beacon_addresses = Vec::new();

    // Look for BeaconCreated events in the logs
    for log in receipt.logs().iter() {
        // Check if this log is from our factory contract
        if log.address() == factory_address {
            // Try to decode as BeaconCreated event
            match log.log_decode::<IBeaconFactory::BeaconCreated>() {
                Ok(decoded_log) => {
                    let beacon = decoded_log.inner.data.beacon;
                    beacon_addresses.push(beacon.to_string());
                    tracing::info!("Parsed BeaconCreated event - beacon address: {}", beacon);
                }
                Err(_) => {
                    // Log is from factory but not BeaconCreated event, continue
                }
            }
        }
    }

    if beacon_addresses.len() as u32 != expected_count {
        return Err(format!(
            "Expected {} BeaconCreated events, but found {}",
            expected_count,
            beacon_addresses.len()
        ));
    }

    Ok(beacon_addresses)
}

/// Parse the PerpCreated event from transaction receipt to get perp ID
///
/// # Arguments
/// * `receipt` - The transaction receipt containing event logs
/// * `perp_hook_address` - The address of the perp hook contract
///
/// # Returns
/// * `Ok(FixedBytes<32>)` - The perp ID from the PerpCreated event
/// * `Err(String)` - Error message if event not found or parsing failed
pub fn parse_perp_created_event(
    receipt: &alloy::rpc::types::TransactionReceipt,
    perp_hook_address: Address,
) -> Result<FixedBytes<32>, String> {
    // Look for the PerpCreated event in the logs
    for log in receipt.logs() {
        // Check if this log is from our perp hook contract
        if log.address() == perp_hook_address {
            // Try to decode as PerpCreated event
            if let Ok(decoded_log) = log.log_decode::<IPerpHook::PerpCreated>() {
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
/// * `perp_hook_address` - The address of the perp hook contract
/// * `expected_perp_id` - The perp ID to match in the event
///
/// # Returns
/// * `Ok(U256)` - The maker position ID from the MakerPositionOpened event
/// * `Err(String)` - Error message if event not found or parsing failed
pub fn parse_maker_position_opened_event(
    receipt: &alloy::rpc::types::TransactionReceipt,
    perp_hook_address: Address,
    expected_perp_id: FixedBytes<32>,
) -> Result<U256, String> {
    // Look for the MakerPositionOpened event in the logs
    for log in receipt.logs() {
        // Check if this log is from our perp hook contract
        if log.address() == perp_hook_address {
            // Try to decode as MakerPositionOpened event
            if let Ok(decoded_log) = log.log_decode::<IPerpHook::MakerPositionOpened>() {
                let event_data = decoded_log.inner.data;

                // Verify this is the event for our perp ID
                if event_data.perpId == expected_perp_id {
                    return Ok(event_data.makerPosId);
                }
            }
        }
    }

    Err("MakerPositionOpened event not found in transaction receipt".to_string())
}

// Tests moved to tests/unit_tests/transaction_events_tests.rs
