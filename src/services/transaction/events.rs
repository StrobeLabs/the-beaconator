use alloy::primitives::{Address, FixedBytes, U256};
use tracing;

use crate::routes::{IBeacon, IPerp, IPerpFactory};

/// Subset of `PerpFactory.PerpCreated` event fields surfaced to API callers.
#[derive(Debug, Clone)]
pub struct PerpCreatedEvent {
    pub perp: Address,
    pub pool_id: FixedBytes<32>,
    pub initial_index: U256,
    pub sqrt_price_x96: U256,
    pub tick: i32,
}

/// Parse the IndexUpdated event from a beacon transaction receipt.
pub fn parse_index_updated_event(
    receipt: &alloy::rpc::types::TransactionReceipt,
    beacon_address: Address,
) -> Result<U256, String> {
    for log in receipt.logs().iter() {
        if log.address() == beacon_address
            && let Ok(decoded_log) = log.log_decode::<IBeacon::IndexUpdated>()
        {
            let index = decoded_log.inner.data.index;
            tracing::info!(
                "Successfully parsed IndexUpdated event - new index: {}",
                index
            );
            return Ok(index);
        }
    }

    let error_msg = "IndexUpdated event not found in transaction receipt";
    tracing::error!("{}", error_msg);
    tracing::error!("Total logs in receipt: {}", receipt.logs().len());
    sentry::capture_message(error_msg, sentry::Level::Error);
    Err(error_msg.to_string())
}

/// Parse the `PerpCreated` event emitted by `PerpFactory.createPerp`. perpcity-contracts@v0.1.0.
pub fn parse_perp_created_event(
    receipt: &alloy::rpc::types::TransactionReceipt,
    perp_factory_address: Address,
) -> Result<PerpCreatedEvent, String> {
    for log in receipt.logs() {
        if log.address() == perp_factory_address
            && let Ok(decoded) = log.log_decode::<IPerpFactory::PerpCreated>()
        {
            let data = decoded.inner.data;
            tracing::info!(
                "Successfully parsed PerpCreated event - perp: {}, pool_id: {}",
                data.perp,
                data.poolId
            );
            return Ok(PerpCreatedEvent {
                perp: data.perp,
                pool_id: data.poolId,
                initial_index: data.initialIndex,
                sqrt_price_x96: U256::from(data.sqrtPriceX96),
                tick: data.tick.as_i32(),
            });
        }
    }

    let msg = "PerpCreated event not found in transaction receipt".to_string();
    tracing::error!("{}", msg);
    sentry::capture_message(&msg, sentry::Level::Error);
    Err(msg)
}

/// Parse the `MakerOpened` event emitted by `Perp.openMaker`. The log emitter is the per-Perp
/// contract address (one Perp per market in v0.1.0), so the caller passes that address.
pub fn parse_maker_opened_event(
    receipt: &alloy::rpc::types::TransactionReceipt,
    perp_address: Address,
) -> Result<U256, String> {
    for log in receipt.logs() {
        if log.address() == perp_address
            && let Ok(decoded) = log.log_decode::<IPerp::MakerOpened>()
        {
            return Ok(decoded.inner.data.posId);
        }
    }

    let msg = "MakerOpened event not found in transaction receipt".to_string();
    tracing::error!("{}", msg);
    sentry::capture_message(&msg, sentry::Level::Error);
    Err(msg)
}

// Tests moved to tests/unit_tests/transaction_events_tests.rs
