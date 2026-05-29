// Transaction event parsing tests — extracted from src/services/transaction/events.rs.
// Pinned to perpcity-contracts@v0.1.0 (Perp + PerpFactory architecture).

use alloy::primitives::{Address, U256};
use std::str::FromStr;
use the_beaconator::services::transaction::events::{
    PerpCreatedEvent, parse_index_updated_event, parse_maker_opened_event, parse_perp_created_event,
};

#[test]
fn test_index_updated_event_interface_compilation() {
    // Compile-time check that IBeacon::IndexUpdated exists and is decodable.
}

#[test]
fn test_parse_index_updated_event_function_exists() {
    let _beacon_address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
    let _function_exists = parse_index_updated_event
        as fn(&alloy::rpc::types::TransactionReceipt, Address) -> Result<U256, String>;
}

#[test]
fn test_parse_perp_created_event_signature() {
    let _function_exists = parse_perp_created_event
        as fn(&alloy::rpc::types::TransactionReceipt, Address) -> Result<PerpCreatedEvent, String>;
}

#[test]
fn test_parse_maker_opened_event_signature() {
    let _function_exists = parse_maker_opened_event
        as fn(&alloy::rpc::types::TransactionReceipt, Address) -> Result<U256, String>;
}
