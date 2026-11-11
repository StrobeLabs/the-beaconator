//! v1 perps (perpcity-contracts v1.0.0, 0df4208).
//!
//! `PerpFactory.createPerp(owner, governor, modules, emaWindow, latency, salt,
//! withdrawalConfig)` reads the beacon with the perp latency (`SafeBeaconIndex.read`), so the
//! beacon must be fresh before the call. The v1 factory has no `perps(address)` mapping;
//! membership is `PerpGuardHook.perps(poolId) == perp`.

use alloy::eips::BlockId;
use alloy::primitives::{Address, B256, Uint};
use alloy::providers::Provider;
use std::time::Duration;
use tokio::time::timeout;

use super::validation::try_decode_revert_reason;
use crate::models::AppState;
use crate::routes::{IPerpFactoryV1, IPerpGuardHookV1, IPerpV1};
use crate::services::beacon::v1::{self as beacon_v1, VerifierKind};
use crate::services::transaction::execution::send_with_nonce_retry;

/// The beacon must still be fresh when createPerp executes, so leave this much time for
/// signing and inclusion.
pub const CREATE_FRESHNESS_MARGIN_SECS: u64 = 120;

/// Smallest latency this route accepts. Plan D1: the 900 s heartbeat needs at least 2820 s
/// (three heartbeat cycles plus a retry margin); markets use max(3600 s, ...).
pub const MIN_V1_LATENCY_SECS: u32 = 2820;

pub struct DeployV1PerpParams {
    pub beacon: Address,
    pub owner: Address,
    pub governor: Address,
    pub ema_window: u32,
    pub latency: u32,
    pub salt: B256,
    pub withdrawal_capacity: u128,
    pub withdrawal_refill_per_second: u128,
    pub withdrawal_challenge_window: u64,
}

pub struct DeployedV1Perp {
    pub perp: Address,
    pub pool_id: B256,
    pub perp_factory: Address,
    pub initial_index: alloy::primitives::U256,
    pub sqrt_price_x96: alloy::primitives::U256,
    pub tick: i32,
    pub tx_hash: B256,
}

/// Input checks that need no chain read.
pub fn validate_params(p: &DeployV1PerpParams) -> Result<(), String> {
    if p.ema_window == 0 || p.ema_window > 0x00FF_FFFF {
        return Err(format!(
            "ema_window {} must be in 1..=16777215 (uint24, non-zero)",
            p.ema_window
        ));
    }
    if p.latency < MIN_V1_LATENCY_SECS {
        return Err(format!(
            "latency {}s is below {MIN_V1_LATENCY_SECS}s: the 900 s heartbeat cannot keep the beacon fresh",
            p.latency
        ));
    }
    if p.withdrawal_challenge_window == 0 || p.withdrawal_challenge_window > 0xFFFF_FFFF_FFFF {
        return Err(format!(
            "withdrawal challenge_window {} must be in 1..=2^48-1 (uint48, non-zero)",
            p.withdrawal_challenge_window
        ));
    }
    if p.owner == Address::ZERO || p.governor == Address::ZERO {
        return Err("owner and governor must be non-zero addresses".to_string());
    }
    Ok(())
}

/// createPerp reads the beacon with the perp latency. Refuse before any gas is spent when
/// the beacon will not still be fresh at inclusion.
pub fn check_fresh_for_create(age_secs: u64, latency: u32) -> Result<(), String> {
    if age_secs.saturating_add(CREATE_FRESHNESS_MARGIN_SECS) > u64::from(latency) {
        return Err(format!(
            "beacon is {age_secs}s old; createPerp needs it younger than latency {latency}s \
             minus {CREATE_FRESHNESS_MARGIN_SECS}s. Push a beacon update first."
        ));
    }
    Ok(())
}

pub async fn deploy_v1_perp_for_beacon(
    state: &AppState,
    p: DeployV1PerpParams,
) -> Result<DeployedV1Perp, String> {
    let v1 = state
        .contracts
        .v1
        .as_ref()
        .ok_or("v1 is not configured: set the V1_* address env vars")?;
    validate_params(&p)?;

    let reading = beacon_v1::read_index(&*state.provider.read_provider, p.beacon).await?;
    let (VerifierKind::V1, Some(updated_at)) = (reading.kind, reading.updated_at) else {
        return Err(format!(
            "beacon {} is an old-kind beacon; a v1 perp needs a v1 beacon",
            p.beacon
        ));
    };
    check_fresh_for_create(
        beacon_v1::age_secs(updated_at, beacon_v1::unix_now_secs()?),
        p.latency,
    )?;

    let handle = state
        .wallets
        .manager
        .acquire_any_wallet()
        .await
        .map_err(|e| format!("Failed to acquire wallet: {e}"))?;
    let provider = handle
        .build_provider(&state.provider.rpc_url)
        .map_err(|e| format!("Failed to build provider: {e}"))?;
    let factory = IPerpFactoryV1::new(v1.perp_factory, &provider);

    let modules = IPerpFactoryV1::Modules {
        beacon: p.beacon,
        fees: v1.fees_module,
        funding: v1.funding_module,
        marginRatios: v1.margin_ratios_module,
        priceImpact: v1.price_impact_module,
        pricing: v1.pricing_module,
    };
    let withdrawal = IPerpFactoryV1::WithdrawalConfig {
        capacity: p.withdrawal_capacity,
        refillPerSecond: p.withdrawal_refill_per_second,
        challengeWindow: Uint::<48, 1>::from(p.withdrawal_challenge_window),
    };
    let ema_window = Uint::<24, 1>::from(p.ema_window);
    let build_call = || {
        factory.createPerp(
            p.owner,
            p.governor,
            modules.clone(),
            ema_window,
            p.latency,
            p.salt,
            withdrawal.clone(),
        )
    };

    // Preflight: StaleBeaconIndex, a used salt or a bad module reverts here, with no gas spent.
    build_call().call().await.map_err(|e| {
        let reason = try_decode_revert_reason(&e).unwrap_or_else(|| e.to_string());
        format!("v1 createPerp preflight reverted: {reason}")
    })?;

    let pending = send_with_nonce_retry("v1 createPerp", || {
        let c = build_call();
        let lock_ok = handle.ensure_lock_held();
        Box::pin(async move {
            lock_ok?;
            c.send()
                .await
                .map_err(|e| format!("v1 createPerp send failed: {e}"))
        })
    })
    .await?;
    let sent_hash = *pending.tx_hash();
    tracing::info!("v1 createPerp tx sent: {sent_hash}");

    let receipt = match timeout(Duration::from_secs(120), pending.get_receipt()).await {
        Ok(Ok(r)) => r,
        _ => super::core::wait_for_receipt(state, sent_hash, "v1 createPerp")
            .await
            .map_err(|e| {
                format!("{e}. The tx was sent: check {sent_hash} on chain before any retry")
            })?,
    };
    if !receipt.status() {
        return Err(format!("v1 createPerp transaction {sent_hash} reverted"));
    }

    let event = receipt
        .logs()
        .iter()
        .filter(|log| log.address() == v1.perp_factory)
        .find_map(|log| log.log_decode::<IPerpFactoryV1::PerpCreated>().ok())
        .ok_or_else(|| format!("PerpCreated event not found in tx {sent_hash}"))?
        .inner
        .data;

    Ok(DeployedV1Perp {
        perp: event.perp,
        pool_id: event.poolId,
        perp_factory: v1.perp_factory,
        initial_index: event.initialIndex,
        sqrt_price_x96: alloy::primitives::U256::from(event.sqrtPriceX96),
        tick: event.tick.as_i32(),
        tx_hash: receipt.transaction_hash,
    })
}

/// Is `perp` a market of the v1 `factory`? Reads the factory's guard hook, the perp's
/// `poolId()`, and `PerpGuardHook.perps(poolId)`. An address with no code is not a member;
/// any other failed read is an error, so a caller never treats an RPC outage as "no".
pub async fn is_v1_perp<P: Provider + Clone>(
    factory: Address,
    provider: P,
    perp: Address,
    block: Option<BlockId>,
) -> Result<bool, alloy::contract::Error> {
    let factory_contract = IPerpFactoryV1::new(factory, provider.clone());
    let mut hook_call = factory_contract.PERP_GUARD_HOOK();
    if let Some(b) = block {
        hook_call = hook_call.block(b);
    }
    let hook = hook_call.call().await?;

    let perp_contract = IPerpV1::new(perp, provider.clone());
    let mut pool_call = perp_contract.poolId();
    if let Some(b) = block {
        pool_call = pool_call.block(b);
    }
    let pool_id = match pool_call.call().await {
        Ok(id) => id,
        Err(e) => {
            if provider.get_code_at(perp).await?.is_empty() {
                return Ok(false);
            }
            return Err(e);
        }
    };

    let hook_contract = IPerpGuardHookV1::new(hook, provider);
    let mut member_call = hook_contract.perps(pool_id);
    if let Some(b) = block {
        member_call = member_call.block(b);
    }
    Ok(member_call.call().await? == perp)
}
