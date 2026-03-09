//! Factory-based beacon creation
//!
//! Creates beacons via on-chain factory contracts (LBCGBMFactory, WeightedSumCompositeFactory).

use alloy::primitives::{Address, U256};
use std::str::FromStr;
use std::time::Duration;
use tokio::time::timeout;

use crate::models::AppState;
use crate::models::beacon_type::BeaconTypeConfig;
use crate::models::requests::{CreateLBCGBMBeaconRequest, CreateWeightedSumCompositeBeaconRequest};
use crate::models::responses::CreateBeaconResponse;
use crate::routes::{ILBCGBMFactory, IWeightedSumCompositeFactory};
use crate::services::beacon::core::{RegistrationOutcome, register_beacon_with_registry};

/// Create an LBCGBM standalone beacon via the on-chain factory.
///
/// Returns the beacon address.
pub async fn create_lbcgbm_beacon(
    state: &AppState,
    config: &BeaconTypeConfig,
    request: &CreateLBCGBMBeaconRequest,
) -> Result<Address, String> {
    let signer_address = state.signer.address();
    tracing::info!(
        "Creating LBCGBM beacon via factory {} with signer={}",
        config.factory_address,
        signer_address
    );

    let wallet_handle = state
        .wallet_manager
        .acquire_any_wallet()
        .await
        .map_err(|e| format!("Failed to acquire wallet: {e}"))?;

    tracing::info!(
        "Acquired wallet {} for LBCGBM beacon creation",
        wallet_handle.address()
    );

    let provider = wallet_handle
        .build_provider(&state.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;

    let factory = ILBCGBMFactory::new(config.factory_address, &provider);

    // Simulate first to get the return address
    let simulated = factory
        .createBeacon(
            signer_address,
            U256::from(request.measurement_scale),
            U256::from(request.sigma_base),
            U256::from(request.scaling_factor),
            U256::from(request.alpha),
            U256::from(request.decay),
            U256::from(request.initial_sigma_ratio),
            request.variance_scaling,
            U256::from(request.min_index),
            U256::from(request.max_index),
            U256::from(request.steepness),
            U256::from(request.initial_index),
        )
        .call()
        .await
        .map_err(|e| format!("Failed to simulate LBCGBM createBeacon: {e}"))?;

    let beacon_address = Address::from(simulated.0);
    tracing::info!(
        "Simulated LBCGBM beacon creation - expected address: {}",
        beacon_address
    );

    // Execute the actual transaction
    let pending_tx = factory
        .createBeacon(
            signer_address,
            U256::from(request.measurement_scale),
            U256::from(request.sigma_base),
            U256::from(request.scaling_factor),
            U256::from(request.alpha),
            U256::from(request.decay),
            U256::from(request.initial_sigma_ratio),
            request.variance_scaling,
            U256::from(request.min_index),
            U256::from(request.max_index),
            U256::from(request.steepness),
            U256::from(request.initial_index),
        )
        .send()
        .await
        .map_err(|e| format!("Failed to send LBCGBM createBeacon transaction: {e}"))?;

    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("LBCGBM beacon creation tx sent: {:?}", tx_hash);

    let receipt = match timeout(Duration::from_secs(120), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => receipt,
        Ok(Err(e)) => return Err(format!("Failed to get LBCGBM beacon creation receipt: {e}")),
        Err(_) => {
            return Err(format!(
                "Timeout waiting for LBCGBM beacon creation receipt (tx: {tx_hash})"
            ));
        }
    };

    if !receipt.status() {
        return Err(format!(
            "LBCGBM beacon creation transaction {tx_hash} reverted"
        ));
    }

    tracing::info!("LBCGBM beacon created at {}", beacon_address);
    sentry::capture_message(
        &format!("LBCGBM beacon created: {beacon_address}"),
        sentry::Level::Info,
    );

    Ok(beacon_address)
}

/// Create a WeightedSumComposite beacon via the on-chain factory.
///
/// Returns the beacon address.
pub async fn create_weighted_sum_composite_beacon(
    state: &AppState,
    config: &BeaconTypeConfig,
    request: &CreateWeightedSumCompositeBeaconRequest,
) -> Result<Address, String> {
    if request.reference_beacons.len() != request.weights.len() {
        return Err(format!(
            "reference_beacons length ({}) must match weights length ({})",
            request.reference_beacons.len(),
            request.weights.len()
        ));
    }

    if request.reference_beacons.is_empty() {
        return Err("reference_beacons must not be empty".to_string());
    }

    let reference_beacons: Vec<Address> = request
        .reference_beacons
        .iter()
        .map(|s| Address::from_str(s).map_err(|e| format!("Invalid reference beacon address: {e}")))
        .collect::<Result<Vec<_>, _>>()?;

    let weights: Vec<U256> = request.weights.iter().map(|w| U256::from(*w)).collect();

    tracing::info!(
        "Creating WeightedSumComposite beacon via factory {} with {} reference beacons",
        config.factory_address,
        reference_beacons.len()
    );

    let wallet_handle = state
        .wallet_manager
        .acquire_any_wallet()
        .await
        .map_err(|e| format!("Failed to acquire wallet: {e}"))?;

    tracing::info!(
        "Acquired wallet {} for composite beacon creation",
        wallet_handle.address()
    );

    let provider = wallet_handle
        .build_provider(&state.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;

    let factory = IWeightedSumCompositeFactory::new(config.factory_address, &provider);

    // Simulate first
    let simulated = factory
        .createBeacon(reference_beacons.clone(), weights.clone())
        .call()
        .await
        .map_err(|e| format!("Failed to simulate WeightedSumComposite createBeacon: {e}"))?;

    let beacon_address = Address::from(simulated.0);
    tracing::info!(
        "Simulated composite beacon creation - expected address: {}",
        beacon_address
    );

    // Execute
    let pending_tx = factory
        .createBeacon(reference_beacons, weights)
        .send()
        .await
        .map_err(|e| {
            format!("Failed to send WeightedSumComposite createBeacon transaction: {e}")
        })?;

    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("Composite beacon creation tx sent: {:?}", tx_hash);

    let receipt = match timeout(Duration::from_secs(120), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => receipt,
        Ok(Err(e)) => {
            return Err(format!(
                "Failed to get composite beacon creation receipt: {e}"
            ));
        }
        Err(_) => {
            return Err(format!(
                "Timeout waiting for composite beacon creation receipt (tx: {tx_hash})"
            ));
        }
    };

    if !receipt.status() {
        return Err(format!(
            "Composite beacon creation transaction {tx_hash} reverted"
        ));
    }

    tracing::info!("WeightedSumComposite beacon created at {}", beacon_address);
    sentry::capture_message(
        &format!("WeightedSumComposite beacon created: {beacon_address}"),
        sentry::Level::Info,
    );

    Ok(beacon_address)
}

/// Create a beacon via factory and optionally register it. Returns CreateBeaconResponse.
pub async fn create_and_register_factory_beacon(
    state: &AppState,
    config: &BeaconTypeConfig,
    beacon_address: Address,
) -> Result<CreateBeaconResponse, String> {
    let (registered, safe_proposal_hash) = if let Some(registry_address) = config.registry_address {
        match register_beacon_with_registry(state, beacon_address, registry_address).await {
            Ok(RegistrationOutcome::OnChainConfirmed(_))
            | Ok(RegistrationOutcome::AlreadyRegistered) => {
                tracing::info!(
                    "Beacon {} registered with registry {}",
                    beacon_address,
                    registry_address
                );
                (true, None)
            }
            Ok(RegistrationOutcome::SafeProposed(hash)) => {
                tracing::info!(
                    "Beacon {} Safe registration proposed (hash: {}), not yet confirmed",
                    beacon_address,
                    hash
                );
                (false, Some(format!("{hash:#x}")))
            }
            Err(e) => {
                tracing::warn!(
                    "Beacon {} created but registration failed: {}",
                    beacon_address,
                    e
                );
                (false, None)
            }
        }
    } else {
        (false, None)
    };

    Ok(CreateBeaconResponse {
        beacon_address: format!("{beacon_address:#x}"),
        beacon_type: config.slug.clone(),
        factory_address: format!("{:#x}", config.factory_address),
        registered,
        safe_proposal_hash,
    })
}
