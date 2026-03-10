//! Modular beacon creation orchestrator
//!
//! Handles multi-step beacon creation by calling individual component factory
//! contracts in sequence, then assembling them into a beacon.

use alloy::primitives::{Address, I256, U256};
use std::str::FromStr;
use std::time::Duration;
use tokio::time::timeout;

use crate::AlloyProvider;
use crate::models::AppState;
use crate::models::component_factory::ComponentFactoryType;
use crate::models::recipe::{
    BaseFnSpec, BeaconKind, BeaconRecipe, ComposerSpec, GroupFnSpec, GroupTransformSpec,
    PreprocessorSpec, TransformSpec,
};
use crate::models::requests::ModularBeaconParams;
use crate::models::responses::BeaconComponentAddresses;
use crate::routes::{
    IArgmaxFactory, IBoundedFactory, ICGBMFactory, ICompositeBeaconFactory,
    IContinuousAllocationFactory, IDGBMFactory, IDiscreteAllocationFactory, IDominanceFactory,
    IEcdsaVerifierFactory, IGMNormalizeFactory, IGroupManagerFactory, IIdentityBeaconFactory,
    IIdentityPreprocessorFactory, IRelativeDominanceFactory, ISoftmaxFactory,
    IStandaloneBeaconFactory, ITernaryToBinaryFactory, IThresholdFactory, IUnboundedFactory,
    IWeightedSumComponentFactory,
};

/// Result of a modular beacon creation
pub struct ModularCreationResult {
    /// Address of the created beacon
    pub beacon_address: Address,
    /// Address of the deployed verifier (if applicable)
    pub verifier_address: Option<Address>,
    /// Addresses of all intermediate components created
    pub components: BeaconComponentAddresses,
}

/// Orchestrates modular beacon creation from a recipe and params.
///
/// Steps:
/// 1. Acquire a wallet from the pool and build a provider
/// 2. Dispatch to the appropriate creation flow based on beacon kind
/// 3. Each flow deploys components via individual factory contracts in sequence
/// 4. Returns the beacon address along with all component addresses
pub async fn create_modular_beacon(
    state: &AppState,
    recipe: &BeaconRecipe,
    params: &ModularBeaconParams,
) -> Result<ModularCreationResult, String> {
    tracing::info!(
        "Starting modular beacon creation for recipe '{}' ({})",
        recipe.slug,
        recipe.name
    );

    // Acquire wallet from pool
    let wallet_handle = state
        .wallet_manager
        .acquire_any_wallet()
        .await
        .map_err(|e| format!("Failed to acquire wallet: {e}"))?;

    tracing::info!(
        "Acquired wallet {} for modular beacon creation (recipe: {})",
        wallet_handle.address(),
        recipe.slug
    );

    // Build provider from wallet handle
    let provider = wallet_handle
        .build_provider(&state.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;

    match &recipe.beacon_kind {
        BeaconKind::Identity => create_identity_beacon_modular(state, params, &provider).await,
        BeaconKind::Standalone {
            preprocessor,
            base_fn,
            transform,
        } => {
            create_standalone_beacon_modular(
                state,
                params,
                &provider,
                preprocessor,
                base_fn,
                transform,
            )
            .await
        }
        BeaconKind::Composite { composer } => {
            create_composite_beacon_modular(state, params, &provider, composer).await
        }
        BeaconKind::Group {
            group_fn,
            group_transform,
        } => create_group_beacon_modular(state, params, &provider, group_fn, group_transform).await,
    }
}

// ---------------------------------------------------------------------------
// Identity beacon
// ---------------------------------------------------------------------------

async fn create_identity_beacon_modular(
    state: &AppState,
    params: &ModularBeaconParams,
    provider: &AlloyProvider,
) -> Result<ModularCreationResult, String> {
    tracing::info!("Creating Identity beacon (verifier + identity beacon)");

    // Step 1: Create ECDSA verifier
    let verifier_addr = create_verifier(state, provider).await?;

    // Step 2: Create identity beacon via factory
    let beacon_factory_addr = state
        .component_factory_registry
        .get_factory_address(&ComponentFactoryType::IdentityBeaconFactory)
        .await?;

    let initial_index = U256::from(
        params
            .initial_index
            .unwrap_or(1_000_000_000_000_000_000u128),
    );

    let factory = IIdentityBeaconFactory::new(beacon_factory_addr, provider);

    // Simulate to get expected address
    let simulated = factory
        .createBeacon(verifier_addr, initial_index)
        .call()
        .await
        .map_err(|e| format!("Failed to simulate identity beacon creation: {e}"))?;
    let beacon_addr = Address::from(simulated.0);
    tracing::info!(
        "Simulated identity beacon creation - expected address: {}",
        beacon_addr
    );

    // Execute actual transaction
    let pending_tx = factory
        .createBeacon(verifier_addr, initial_index)
        .send()
        .await
        .map_err(|e| format!("Failed to send identity beacon creation transaction: {e}"))?;
    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("Identity beacon creation tx sent: {:?}", tx_hash);

    wait_for_receipt("identity beacon creation", tx_hash, pending_tx).await?;

    tracing::info!("Identity beacon created at {}", beacon_addr);
    sentry::capture_message(
        &format!(
            "Modular Identity beacon created: {} (verifier: {})",
            beacon_addr, verifier_addr
        ),
        sentry::Level::Info,
    );

    Ok(ModularCreationResult {
        beacon_address: beacon_addr,
        verifier_address: Some(verifier_addr),
        components: BeaconComponentAddresses::default(),
    })
}

// ---------------------------------------------------------------------------
// Standalone beacon
// ---------------------------------------------------------------------------

async fn create_standalone_beacon_modular(
    state: &AppState,
    params: &ModularBeaconParams,
    provider: &AlloyProvider,
    preprocessor_spec: &PreprocessorSpec,
    base_fn_spec: &BaseFnSpec,
    transform_spec: &TransformSpec,
) -> Result<ModularCreationResult, String> {
    tracing::info!(
        "Creating Standalone beacon (verifier + {:?} + {:?} + {:?})",
        preprocessor_spec,
        base_fn_spec,
        transform_spec
    );

    // Step 1: Create ECDSA verifier
    let verifier_addr = create_verifier(state, provider).await?;

    // Step 2: Create preprocessor
    let preprocessor_addr = create_preprocessor(state, params, provider, preprocessor_spec).await?;

    // Step 3: Create base function
    let basefn_addr = create_base_fn(state, params, provider, base_fn_spec).await?;

    // Step 4: Create transform
    let transform_addr = create_transform(state, params, provider, transform_spec).await?;

    // Step 5: Create standalone beacon
    let beacon_factory_addr = state
        .component_factory_registry
        .get_factory_address(&ComponentFactoryType::StandaloneBeaconFactory)
        .await?;

    let initial_index = U256::from(
        params
            .initial_index
            .unwrap_or(1_000_000_000_000_000_000u128),
    );

    let factory = IStandaloneBeaconFactory::new(beacon_factory_addr, provider);

    // Simulate
    let simulated = factory
        .createBeacon(
            verifier_addr,
            preprocessor_addr,
            basefn_addr,
            transform_addr,
            initial_index,
        )
        .call()
        .await
        .map_err(|e| format!("Failed to simulate standalone beacon creation: {e}"))?;
    let beacon_addr = Address::from(simulated.0);
    tracing::info!(
        "Simulated standalone beacon creation - expected address: {}",
        beacon_addr
    );

    // Execute
    let pending_tx = factory
        .createBeacon(
            verifier_addr,
            preprocessor_addr,
            basefn_addr,
            transform_addr,
            initial_index,
        )
        .send()
        .await
        .map_err(|e| format!("Failed to send standalone beacon creation transaction: {e}"))?;
    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("Standalone beacon creation tx sent: {:?}", tx_hash);

    wait_for_receipt("standalone beacon creation", tx_hash, pending_tx).await?;

    tracing::info!("Standalone beacon created at {}", beacon_addr);
    sentry::capture_message(
        &format!("Modular Standalone beacon created: {}", beacon_addr),
        sentry::Level::Info,
    );

    Ok(ModularCreationResult {
        beacon_address: beacon_addr,
        verifier_address: Some(verifier_addr),
        components: BeaconComponentAddresses {
            preprocessor: Some(format!("{preprocessor_addr:#x}")),
            base_fn: Some(format!("{basefn_addr:#x}")),
            transform: Some(format!("{transform_addr:#x}")),
            ..Default::default()
        },
    })
}

// ---------------------------------------------------------------------------
// Composite beacon
// ---------------------------------------------------------------------------

async fn create_composite_beacon_modular(
    state: &AppState,
    params: &ModularBeaconParams,
    provider: &AlloyProvider,
    composer_spec: &ComposerSpec,
) -> Result<ModularCreationResult, String> {
    tracing::info!("Creating Composite beacon ({:?})", composer_spec);

    // Step 1: Create composer
    let composer_addr = create_composer(state, params, provider, composer_spec).await?;

    // Step 2: Parse reference beacons
    let reference_beacon_strs = params
        .reference_beacons
        .as_ref()
        .ok_or("Missing required parameter: reference_beacons")?;

    if reference_beacon_strs.is_empty() {
        return Err("reference_beacons must not be empty".to_string());
    }

    let reference_beacons: Vec<Address> = reference_beacon_strs
        .iter()
        .map(|s| {
            Address::from_str(s)
                .map_err(|e| format!("Invalid reference beacon address '{}': {e}", s))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Step 3: Create composite beacon
    let beacon_factory_addr = state
        .component_factory_registry
        .get_factory_address(&ComponentFactoryType::CompositeBeaconFactory)
        .await?;

    let factory = ICompositeBeaconFactory::new(beacon_factory_addr, provider);

    // Simulate
    let simulated = factory
        .createBeacon(reference_beacons.clone(), composer_addr)
        .call()
        .await
        .map_err(|e| format!("Failed to simulate composite beacon creation: {e}"))?;
    let beacon_addr = Address::from(simulated.0);
    tracing::info!(
        "Simulated composite beacon creation - expected address: {}",
        beacon_addr
    );

    // Execute
    let pending_tx = factory
        .createBeacon(reference_beacons, composer_addr)
        .send()
        .await
        .map_err(|e| format!("Failed to send composite beacon creation transaction: {e}"))?;
    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("Composite beacon creation tx sent: {:?}", tx_hash);

    wait_for_receipt("composite beacon creation", tx_hash, pending_tx).await?;

    tracing::info!("Composite beacon created at {}", beacon_addr);
    sentry::capture_message(
        &format!("Modular Composite beacon created: {}", beacon_addr),
        sentry::Level::Info,
    );

    Ok(ModularCreationResult {
        beacon_address: beacon_addr,
        verifier_address: None,
        components: BeaconComponentAddresses {
            composer: Some(format!("{composer_addr:#x}")),
            ..Default::default()
        },
    })
}

// ---------------------------------------------------------------------------
// Group beacon
// ---------------------------------------------------------------------------

async fn create_group_beacon_modular(
    state: &AppState,
    params: &ModularBeaconParams,
    provider: &AlloyProvider,
    group_fn_spec: &GroupFnSpec,
    group_transform_spec: &GroupTransformSpec,
) -> Result<ModularCreationResult, String> {
    tracing::info!(
        "Creating Group beacon ({:?} + {:?})",
        group_fn_spec,
        group_transform_spec
    );

    // Step 1: Create ECDSA verifier
    let verifier_addr = create_verifier(state, provider).await?;

    // Step 2: Create group function
    let groupfn_addr = create_group_fn(state, params, provider, group_fn_spec).await?;

    // Step 3: Create group transform
    let grouptransform_addr =
        create_group_transform(state, params, provider, group_transform_spec).await?;

    // Step 4: Create group manager
    let beacon_factory_addr = state
        .component_factory_registry
        .get_factory_address(&ComponentFactoryType::GroupManagerFactory)
        .await?;

    let initial_indices_raw = require_param_vec(&params.initial_indices, "initial_indices")?;
    let initial_indices: Vec<U256> = initial_indices_raw.iter().map(|v| U256::from(*v)).collect();

    let initial_z_raw =
        require_param_vec(&params.initial_z_space_indices, "initial_z_space_indices")?;
    let initial_z_space_indices: Vec<I256> = initial_z_raw
        .iter()
        .map(|v| {
            if *v >= 0 {
                I256::try_from(*v as u128).unwrap_or(I256::ZERO)
            } else {
                // For negative values: negate the absolute value
                let abs_val = v.unsigned_abs();
                -I256::try_from(abs_val).unwrap_or(I256::ZERO)
            }
        })
        .collect();

    let factory = IGroupManagerFactory::new(beacon_factory_addr, provider);

    // Simulate
    let simulated = factory
        .createGroupManager(
            initial_indices.clone(),
            initial_z_space_indices.clone(),
            verifier_addr,
            groupfn_addr,
            grouptransform_addr,
        )
        .call()
        .await
        .map_err(|e| format!("Failed to simulate group manager creation: {e}"))?;
    let beacon_addr = Address::from(simulated.0);
    tracing::info!(
        "Simulated group manager creation - expected address: {}",
        beacon_addr
    );

    // Execute
    let pending_tx = factory
        .createGroupManager(
            initial_indices,
            initial_z_space_indices,
            verifier_addr,
            groupfn_addr,
            grouptransform_addr,
        )
        .send()
        .await
        .map_err(|e| format!("Failed to send group manager creation transaction: {e}"))?;
    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("Group manager creation tx sent: {:?}", tx_hash);

    wait_for_receipt("group manager creation", tx_hash, pending_tx).await?;

    tracing::info!("Group manager created at {}", beacon_addr);
    sentry::capture_message(
        &format!("Modular Group beacon created: {}", beacon_addr),
        sentry::Level::Info,
    );

    Ok(ModularCreationResult {
        beacon_address: beacon_addr,
        verifier_address: Some(verifier_addr),
        components: BeaconComponentAddresses {
            group_fn: Some(format!("{groupfn_addr:#x}")),
            group_transform: Some(format!("{grouptransform_addr:#x}")),
            ..Default::default()
        },
    })
}

// ---------------------------------------------------------------------------
// Component creators
// ---------------------------------------------------------------------------

/// Create an ECDSA verifier via the ECDSAVerifierFactory.
async fn create_verifier(state: &AppState, provider: &AlloyProvider) -> Result<Address, String> {
    let signer_address = state.signer.address();
    tracing::info!(
        "Creating ECDSAVerifier via factory with signer={}",
        signer_address
    );

    let verifier_factory_addr = state
        .component_factory_registry
        .get_factory_address(&ComponentFactoryType::ECDSAVerifierFactory)
        .await?;

    let factory = IEcdsaVerifierFactory::new(verifier_factory_addr, provider);

    // Simulate
    let simulated = factory
        .createVerifier(signer_address)
        .call()
        .await
        .map_err(|e| format!("Failed to simulate ECDSA verifier creation: {e}"))?;
    let verifier_addr = Address::from(simulated.0);
    tracing::info!(
        "Simulated ECDSA verifier creation - expected address: {}",
        verifier_addr
    );

    // Execute
    let pending_tx = factory
        .createVerifier(signer_address)
        .send()
        .await
        .map_err(|e| format!("Failed to send ECDSA verifier creation transaction: {e}"))?;
    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("ECDSA verifier creation tx sent: {:?}", tx_hash);

    wait_for_receipt("ECDSA verifier creation", tx_hash, pending_tx).await?;

    tracing::info!("ECDSAVerifier created at {}", verifier_addr);
    Ok(verifier_addr)
}

/// Create a preprocessor component via the appropriate factory.
async fn create_preprocessor(
    state: &AppState,
    params: &ModularBeaconParams,
    provider: &AlloyProvider,
    spec: &PreprocessorSpec,
) -> Result<Address, String> {
    let factory_addr = state
        .component_factory_registry
        .get_factory_address(&spec.factory_type())
        .await?;

    let measurement_scale = U256::from(require_param(
        &params.measurement_scale,
        "measurement_scale",
    )?);

    let addr = match spec {
        PreprocessorSpec::Identity => {
            tracing::info!("Creating Identity preprocessor");
            let factory = IIdentityPreprocessorFactory::new(factory_addr, provider);

            let simulated = factory
                .createPreprocessor(measurement_scale)
                .call()
                .await
                .map_err(|e| format!("Failed to simulate identity preprocessor creation: {e}"))?;
            let addr = Address::from(simulated.0);
            tracing::info!(
                "Simulated identity preprocessor creation - expected address: {}",
                addr
            );

            let pending_tx = factory
                .createPreprocessor(measurement_scale)
                .send()
                .await
                .map_err(|e| {
                    format!("Failed to send identity preprocessor creation transaction: {e}")
                })?;
            let tx_hash = *pending_tx.tx_hash();
            tracing::info!("Identity preprocessor creation tx sent: {:?}", tx_hash);

            wait_for_receipt("identity preprocessor creation", tx_hash, pending_tx).await?;
            addr
        }
        PreprocessorSpec::Threshold => {
            let threshold_val = U256::from(require_param(&params.threshold, "threshold")?);
            tracing::info!("Creating Threshold preprocessor");
            let factory = IThresholdFactory::new(factory_addr, provider);

            let simulated = factory
                .createPreprocessor(measurement_scale, threshold_val)
                .call()
                .await
                .map_err(|e| format!("Failed to simulate threshold preprocessor creation: {e}"))?;
            let addr = Address::from(simulated.0);
            tracing::info!(
                "Simulated threshold preprocessor creation - expected address: {}",
                addr
            );

            let pending_tx = factory
                .createPreprocessor(measurement_scale, threshold_val)
                .send()
                .await
                .map_err(|e| {
                    format!("Failed to send threshold preprocessor creation transaction: {e}")
                })?;
            let tx_hash = *pending_tx.tx_hash();
            tracing::info!("Threshold preprocessor creation tx sent: {:?}", tx_hash);

            wait_for_receipt("threshold preprocessor creation", tx_hash, pending_tx).await?;
            addr
        }
        PreprocessorSpec::TernaryToBinary => {
            let threshold_val = U256::from(require_param(&params.threshold, "threshold")?);
            tracing::info!("Creating TernaryToBinary preprocessor");
            let factory = ITernaryToBinaryFactory::new(factory_addr, provider);

            let simulated = factory
                .createPreprocessor(measurement_scale, threshold_val)
                .call()
                .await
                .map_err(|e| {
                    format!("Failed to simulate ternary-to-binary preprocessor creation: {e}")
                })?;
            let addr = Address::from(simulated.0);
            tracing::info!(
                "Simulated ternary-to-binary preprocessor creation - expected address: {}",
                addr
            );

            let pending_tx = factory
                .createPreprocessor(measurement_scale, threshold_val)
                .send()
                .await
                .map_err(|e| {
                    format!(
                        "Failed to send ternary-to-binary preprocessor creation transaction: {e}"
                    )
                })?;
            let tx_hash = *pending_tx.tx_hash();
            tracing::info!(
                "TernaryToBinary preprocessor creation tx sent: {:?}",
                tx_hash
            );

            wait_for_receipt(
                "ternary-to-binary preprocessor creation",
                tx_hash,
                pending_tx,
            )
            .await?;
            addr
        }
        PreprocessorSpec::Argmax => {
            tracing::info!("Creating Argmax preprocessor");
            let factory = IArgmaxFactory::new(factory_addr, provider);

            let simulated = factory
                .createPreprocessor(measurement_scale)
                .call()
                .await
                .map_err(|e| format!("Failed to simulate argmax preprocessor creation: {e}"))?;
            let addr = Address::from(simulated.0);
            tracing::info!(
                "Simulated argmax preprocessor creation - expected address: {}",
                addr
            );

            let pending_tx = factory
                .createPreprocessor(measurement_scale)
                .send()
                .await
                .map_err(|e| {
                    format!("Failed to send argmax preprocessor creation transaction: {e}")
                })?;
            let tx_hash = *pending_tx.tx_hash();
            tracing::info!("Argmax preprocessor creation tx sent: {:?}", tx_hash);

            wait_for_receipt("argmax preprocessor creation", tx_hash, pending_tx).await?;
            addr
        }
    };

    tracing::info!("Preprocessor ({:?}) created at {}", spec, addr);
    Ok(addr)
}

/// Create a base function component via the appropriate factory.
async fn create_base_fn(
    state: &AppState,
    params: &ModularBeaconParams,
    provider: &AlloyProvider,
    spec: &BaseFnSpec,
) -> Result<Address, String> {
    let factory_addr = state
        .component_factory_registry
        .get_factory_address(&spec.factory_type())
        .await?;

    let addr = match spec {
        BaseFnSpec::CGBM => {
            let sigma_base = U256::from(require_param(&params.sigma_base, "sigma_base")?);
            let scaling_factor =
                U256::from(require_param(&params.scaling_factor, "scaling_factor")?);
            let alpha = U256::from(require_param(&params.alpha, "alpha")?);
            let decay = U256::from(require_param(&params.decay, "decay")?);
            let initial_sigma_ratio = U256::from(require_param(
                &params.initial_sigma_ratio,
                "initial_sigma_ratio",
            )?);
            let variance_scaling = params.variance_scaling.unwrap_or(false);

            tracing::info!("Creating CGBM base function");
            let factory = ICGBMFactory::new(factory_addr, provider);

            let simulated = factory
                .createBaseFn(
                    sigma_base,
                    scaling_factor,
                    alpha,
                    decay,
                    initial_sigma_ratio,
                    variance_scaling,
                )
                .call()
                .await
                .map_err(|e| format!("Failed to simulate CGBM base function creation: {e}"))?;
            let addr = Address::from(simulated.0);
            tracing::info!(
                "Simulated CGBM base function creation - expected address: {}",
                addr
            );

            let pending_tx = factory
                .createBaseFn(
                    sigma_base,
                    scaling_factor,
                    alpha,
                    decay,
                    initial_sigma_ratio,
                    variance_scaling,
                )
                .send()
                .await
                .map_err(|e| {
                    format!("Failed to send CGBM base function creation transaction: {e}")
                })?;
            let tx_hash = *pending_tx.tx_hash();
            tracing::info!("CGBM base function creation tx sent: {:?}", tx_hash);

            wait_for_receipt("CGBM base function creation", tx_hash, pending_tx).await?;
            addr
        }
        BaseFnSpec::DGBM => {
            let sigma_base = U256::from(require_param(&params.sigma_base, "sigma_base")?);
            let scaling_factor =
                U256::from(require_param(&params.scaling_factor, "scaling_factor")?);
            let decay = U256::from(require_param(&params.decay, "decay")?);
            let initial_positive_rate = U256::from(require_param(
                &params.initial_positive_rate,
                "initial_positive_rate",
            )?);

            tracing::info!("Creating DGBM base function");
            let factory = IDGBMFactory::new(factory_addr, provider);

            let simulated = factory
                .createBaseFn(sigma_base, scaling_factor, decay, initial_positive_rate)
                .call()
                .await
                .map_err(|e| format!("Failed to simulate DGBM base function creation: {e}"))?;
            let addr = Address::from(simulated.0);
            tracing::info!(
                "Simulated DGBM base function creation - expected address: {}",
                addr
            );

            let pending_tx = factory
                .createBaseFn(sigma_base, scaling_factor, decay, initial_positive_rate)
                .send()
                .await
                .map_err(|e| {
                    format!("Failed to send DGBM base function creation transaction: {e}")
                })?;
            let tx_hash = *pending_tx.tx_hash();
            tracing::info!("DGBM base function creation tx sent: {:?}", tx_hash);

            wait_for_receipt("DGBM base function creation", tx_hash, pending_tx).await?;
            addr
        }
    };

    tracing::info!("BaseFn ({:?}) created at {}", spec, addr);
    Ok(addr)
}

/// Create a transform component via the appropriate factory.
async fn create_transform(
    state: &AppState,
    params: &ModularBeaconParams,
    provider: &AlloyProvider,
    spec: &TransformSpec,
) -> Result<Address, String> {
    let factory_addr = state
        .component_factory_registry
        .get_factory_address(&spec.factory_type())
        .await?;

    let addr = match spec {
        TransformSpec::Bounded => {
            let min_index = U256::from(require_param(&params.min_index, "min_index")?);
            let max_index = U256::from(require_param(&params.max_index, "max_index")?);
            let steepness = U256::from(require_param(&params.steepness, "steepness")?);

            tracing::info!("Creating Bounded transform");
            let factory = IBoundedFactory::new(factory_addr, provider);

            let simulated = factory
                .createTransform(min_index, max_index, steepness)
                .call()
                .await
                .map_err(|e| format!("Failed to simulate bounded transform creation: {e}"))?;
            let addr = Address::from(simulated.0);
            tracing::info!(
                "Simulated bounded transform creation - expected address: {}",
                addr
            );

            let pending_tx = factory
                .createTransform(min_index, max_index, steepness)
                .send()
                .await
                .map_err(|e| {
                    format!("Failed to send bounded transform creation transaction: {e}")
                })?;
            let tx_hash = *pending_tx.tx_hash();
            tracing::info!("Bounded transform creation tx sent: {:?}", tx_hash);

            wait_for_receipt("bounded transform creation", tx_hash, pending_tx).await?;
            addr
        }
        TransformSpec::Unbounded => {
            let initial_index = U256::from(require_param(
                &params.initial_index,
                "initial_index (for unbounded transform)",
            )?);

            tracing::info!("Creating Unbounded transform");
            let factory = IUnboundedFactory::new(factory_addr, provider);

            let simulated = factory
                .createTransform(initial_index)
                .call()
                .await
                .map_err(|e| format!("Failed to simulate unbounded transform creation: {e}"))?;
            let addr = Address::from(simulated.0);
            tracing::info!(
                "Simulated unbounded transform creation - expected address: {}",
                addr
            );

            let pending_tx = factory
                .createTransform(initial_index)
                .send()
                .await
                .map_err(|e| {
                    format!("Failed to send unbounded transform creation transaction: {e}")
                })?;
            let tx_hash = *pending_tx.tx_hash();
            tracing::info!("Unbounded transform creation tx sent: {:?}", tx_hash);

            wait_for_receipt("unbounded transform creation", tx_hash, pending_tx).await?;
            addr
        }
    };

    tracing::info!("Transform ({:?}) created at {}", spec, addr);
    Ok(addr)
}

/// Create a composer component via the appropriate factory.
async fn create_composer(
    state: &AppState,
    params: &ModularBeaconParams,
    provider: &AlloyProvider,
    spec: &ComposerSpec,
) -> Result<Address, String> {
    let factory_addr = state
        .component_factory_registry
        .get_factory_address(&spec.factory_type())
        .await?;

    let addr = match spec {
        ComposerSpec::WeightedSum => {
            let weights_raw = require_param_vec(&params.weights, "weights")?;
            let weights: Vec<U256> = weights_raw.iter().map(|w| U256::from(*w)).collect();

            tracing::info!(
                "Creating WeightedSum composer with {} weights",
                weights.len()
            );
            let factory = IWeightedSumComponentFactory::new(factory_addr, provider);

            let simulated = factory
                .createComposer(weights.clone())
                .call()
                .await
                .map_err(|e| format!("Failed to simulate weighted sum composer creation: {e}"))?;
            let addr = Address::from(simulated.0);
            tracing::info!(
                "Simulated weighted sum composer creation - expected address: {}",
                addr
            );

            let pending_tx = factory.createComposer(weights).send().await.map_err(|e| {
                format!("Failed to send weighted sum composer creation transaction: {e}")
            })?;
            let tx_hash = *pending_tx.tx_hash();
            tracing::info!("WeightedSum composer creation tx sent: {:?}", tx_hash);

            wait_for_receipt("weighted sum composer creation", tx_hash, pending_tx).await?;
            addr
        }
    };

    tracing::info!("Composer ({:?}) created at {}", spec, addr);
    Ok(addr)
}

/// Create a group function component via the appropriate factory.
async fn create_group_fn(
    state: &AppState,
    params: &ModularBeaconParams,
    provider: &AlloyProvider,
    spec: &GroupFnSpec,
) -> Result<Address, String> {
    let factory_addr = state
        .component_factory_registry
        .get_factory_address(&spec.factory_type())
        .await?;

    let addr = match spec {
        GroupFnSpec::Dominance => {
            let num_classes = U256::from(require_param(&params.num_classes, "num_classes")?);
            let alpha = U256::from(require_param(&params.alpha, "alpha")?);
            let decay = U256::from(require_param(&params.decay, "decay")?);
            let initial_ema_raw = require_param_vec(&params.initial_ema, "initial_ema")?;
            let initial_ema: Vec<U256> = initial_ema_raw.iter().map(|v| U256::from(*v)).collect();

            tracing::info!("Creating Dominance group function");
            let factory = IDominanceFactory::new(factory_addr, provider);

            let simulated = factory
                .createGroupFn(num_classes, alpha, decay, initial_ema.clone())
                .call()
                .await
                .map_err(|e| {
                    format!("Failed to simulate dominance group function creation: {e}")
                })?;
            let addr = Address::from(simulated.0);
            tracing::info!(
                "Simulated dominance group function creation - expected address: {}",
                addr
            );

            let pending_tx = factory
                .createGroupFn(num_classes, alpha, decay, initial_ema)
                .send()
                .await
                .map_err(|e| {
                    format!("Failed to send dominance group function creation transaction: {e}")
                })?;
            let tx_hash = *pending_tx.tx_hash();
            tracing::info!("Dominance group function creation tx sent: {:?}", tx_hash);

            wait_for_receipt("dominance group function creation", tx_hash, pending_tx).await?;
            addr
        }
        GroupFnSpec::RelativeDominance => {
            let num_classes = U256::from(require_param(&params.num_classes, "num_classes")?);
            let alpha = U256::from(require_param(&params.alpha, "alpha")?);
            let decay_fast = U256::from(require_param(&params.decay_fast, "decay_fast")?);
            let decay_slow = U256::from(require_param(&params.decay_slow, "decay_slow")?);
            let initial_m_fast_raw = require_param_vec(&params.initial_m_fast, "initial_m_fast")?;
            let initial_m_fast: Vec<U256> =
                initial_m_fast_raw.iter().map(|v| U256::from(*v)).collect();
            let initial_m_slow_raw = require_param_vec(&params.initial_m_slow, "initial_m_slow")?;
            let initial_m_slow: Vec<U256> =
                initial_m_slow_raw.iter().map(|v| U256::from(*v)).collect();

            tracing::info!("Creating RelativeDominance group function");
            let factory = IRelativeDominanceFactory::new(factory_addr, provider);

            let simulated = factory
                .createGroupFn(
                    num_classes,
                    alpha,
                    decay_fast,
                    decay_slow,
                    initial_m_fast.clone(),
                    initial_m_slow.clone(),
                )
                .call()
                .await
                .map_err(|e| {
                    format!("Failed to simulate relative dominance group function creation: {e}")
                })?;
            let addr = Address::from(simulated.0);
            tracing::info!(
                "Simulated relative dominance group function creation - expected address: {}",
                addr
            );

            let pending_tx = factory
                .createGroupFn(
                    num_classes,
                    alpha,
                    decay_fast,
                    decay_slow,
                    initial_m_fast,
                    initial_m_slow,
                )
                .send()
                .await
                .map_err(|e| {
                    format!(
                        "Failed to send relative dominance group function creation transaction: {e}"
                    )
                })?;
            let tx_hash = *pending_tx.tx_hash();
            tracing::info!(
                "RelativeDominance group function creation tx sent: {:?}",
                tx_hash
            );

            wait_for_receipt(
                "relative dominance group function creation",
                tx_hash,
                pending_tx,
            )
            .await?;
            addr
        }
        GroupFnSpec::ContinuousAllocation => {
            let class_probs_raw = require_param_vec(&params.class_probs, "class_probs")?;
            let class_probs: Vec<U256> = class_probs_raw.iter().map(|v| U256::from(*v)).collect();
            let sigma_base = U256::from(require_param(&params.sigma_base, "sigma_base")?);
            let scale_factor = U256::from(require_param(&params.scaling_factor, "scaling_factor")?);
            let decay = U256::from(require_param(&params.decay, "decay")?);

            tracing::info!("Creating ContinuousAllocation group function");
            let factory = IContinuousAllocationFactory::new(factory_addr, provider);

            let simulated = factory
                .createGroupFn(class_probs.clone(), sigma_base, scale_factor, decay)
                .call()
                .await
                .map_err(|e| {
                    format!("Failed to simulate continuous allocation group function creation: {e}")
                })?;
            let addr = Address::from(simulated.0);
            tracing::info!(
                "Simulated continuous allocation group function creation - expected address: {}",
                addr
            );

            let pending_tx = factory
                .createGroupFn(class_probs, sigma_base, scale_factor, decay)
                .send()
                .await
                .map_err(|e| {
                    format!(
                        "Failed to send continuous allocation group function creation transaction: {e}"
                    )
                })?;
            let tx_hash = *pending_tx.tx_hash();
            tracing::info!(
                "ContinuousAllocation group function creation tx sent: {:?}",
                tx_hash
            );

            wait_for_receipt(
                "continuous allocation group function creation",
                tx_hash,
                pending_tx,
            )
            .await?;
            addr
        }
        GroupFnSpec::DiscreteAllocation => {
            let class_probs_raw = require_param_vec(&params.class_probs, "class_probs")?;
            let class_probs: Vec<U256> = class_probs_raw.iter().map(|v| U256::from(*v)).collect();
            let sigma_base = U256::from(require_param(&params.sigma_base, "sigma_base")?);
            let scale_factor = U256::from(require_param(&params.scaling_factor, "scaling_factor")?);
            let decay = U256::from(require_param(&params.decay, "decay")?);

            tracing::info!("Creating DiscreteAllocation group function");
            let factory = IDiscreteAllocationFactory::new(factory_addr, provider);

            let simulated = factory
                .createGroupFn(class_probs.clone(), sigma_base, scale_factor, decay)
                .call()
                .await
                .map_err(|e| {
                    format!("Failed to simulate discrete allocation group function creation: {e}")
                })?;
            let addr = Address::from(simulated.0);
            tracing::info!(
                "Simulated discrete allocation group function creation - expected address: {}",
                addr
            );

            let pending_tx = factory
                .createGroupFn(class_probs, sigma_base, scale_factor, decay)
                .send()
                .await
                .map_err(|e| {
                    format!(
                        "Failed to send discrete allocation group function creation transaction: {e}"
                    )
                })?;
            let tx_hash = *pending_tx.tx_hash();
            tracing::info!(
                "DiscreteAllocation group function creation tx sent: {:?}",
                tx_hash
            );

            wait_for_receipt(
                "discrete allocation group function creation",
                tx_hash,
                pending_tx,
            )
            .await?;
            addr
        }
    };

    tracing::info!("GroupFn ({:?}) created at {}", spec, addr);
    Ok(addr)
}

/// Create a group transform component via the appropriate factory.
async fn create_group_transform(
    state: &AppState,
    params: &ModularBeaconParams,
    provider: &AlloyProvider,
    spec: &GroupTransformSpec,
) -> Result<Address, String> {
    let factory_addr = state
        .component_factory_registry
        .get_factory_address(&spec.factory_type())
        .await?;

    let addr = match spec {
        GroupTransformSpec::Softmax => {
            let steepness = U256::from(require_param(&params.steepness, "steepness")?);
            let index_scale = U256::from(require_param(&params.index_scale, "index_scale")?);

            tracing::info!("Creating Softmax group transform");
            let factory = ISoftmaxFactory::new(factory_addr, provider);

            let simulated = factory
                .createGroupTransform(steepness, index_scale)
                .call()
                .await
                .map_err(|e| format!("Failed to simulate softmax group transform creation: {e}"))?;
            let addr = Address::from(simulated.0);
            tracing::info!(
                "Simulated softmax group transform creation - expected address: {}",
                addr
            );

            let pending_tx = factory
                .createGroupTransform(steepness, index_scale)
                .send()
                .await
                .map_err(|e| {
                    format!("Failed to send softmax group transform creation transaction: {e}")
                })?;
            let tx_hash = *pending_tx.tx_hash();
            tracing::info!("Softmax group transform creation tx sent: {:?}", tx_hash);

            wait_for_receipt("softmax group transform creation", tx_hash, pending_tx).await?;
            addr
        }
        GroupTransformSpec::GMNormalize => {
            let index_scale = U256::from(require_param(&params.index_scale, "index_scale")?);

            tracing::info!("Creating GMNormalize group transform");
            let factory = IGMNormalizeFactory::new(factory_addr, provider);

            let simulated = factory
                .createGroupTransform(index_scale)
                .call()
                .await
                .map_err(|e| {
                    format!("Failed to simulate gm-normalize group transform creation: {e}")
                })?;
            let addr = Address::from(simulated.0);
            tracing::info!(
                "Simulated gm-normalize group transform creation - expected address: {}",
                addr
            );

            let pending_tx = factory
                .createGroupTransform(index_scale)
                .send()
                .await
                .map_err(|e| {
                    format!("Failed to send gm-normalize group transform creation transaction: {e}")
                })?;
            let tx_hash = *pending_tx.tx_hash();
            tracing::info!(
                "GMNormalize group transform creation tx sent: {:?}",
                tx_hash
            );

            wait_for_receipt("gm-normalize group transform creation", tx_hash, pending_tx).await?;
            addr
        }
    };

    tracing::info!("GroupTransform ({:?}) created at {}", spec, addr);
    Ok(addr)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract a required scalar parameter or return a descriptive error.
fn require_param<T: Copy>(val: &Option<T>, name: &str) -> Result<T, String> {
    val.ok_or_else(|| format!("Missing required parameter: {name}"))
}

/// Extract a required vector parameter or return a descriptive error.
fn require_param_vec<T: Clone>(val: &Option<Vec<T>>, name: &str) -> Result<Vec<T>, String> {
    val.as_ref()
        .cloned()
        .ok_or_else(|| format!("Missing required parameter: {name}"))
}

/// Wait for a pending transaction receipt with a 120-second timeout.
///
/// Checks the receipt status and returns an error if the transaction reverted.
async fn wait_for_receipt(
    description: &str,
    tx_hash: alloy::primitives::TxHash,
    pending_tx: alloy::providers::PendingTransactionBuilder<alloy::network::Ethereum>,
) -> Result<(), String> {
    let receipt = match timeout(Duration::from_secs(120), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => receipt,
        Ok(Err(e)) => {
            return Err(format!("Failed to get {} receipt: {e}", description));
        }
        Err(_) => {
            return Err(format!(
                "Timeout waiting for {} receipt (tx: {tx_hash})",
                description
            ));
        }
    };

    if !receipt.status() {
        return Err(format!("{} transaction {tx_hash} reverted", description));
    }

    Ok(())
}
