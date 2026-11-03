//! Read and prepare controller operations. No route signs or executes guardian calls.
use crate::{
    guards::{AdminToken, ApiToken},
    models::AppState,
};
use alloy::{
    eips::BlockId,
    primitives::{Address, B256, U256},
    providers::Provider,
    sol,
    sol_types::SolCall,
};
use rocket::{State, http::Status, post, serde::json::Json};
use rocket_okapi::openapi;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

sol! {
    #[sol(rpc)]
    #[allow(clippy::too_many_arguments)]
    interface ProtectedFactory {
        struct Modules { address beacon; address fees; address funding; address marginRatios; address priceImpact; address pricing; }
        struct WithdrawalConfig { uint128 capacity; uint128 refillPerSecond; uint48 challengeWindow; }
        function createPerp(address owner, address governor, Modules modules, uint24 emaWindow, uint32 latency, bytes32 salt, WithdrawalConfig withdrawalConfig) external returns (address);
        event PerpCreated(address perp, bytes32 poolId, Modules modules, uint256 initialIndex, uint24 emaWindow, uint32 latency, uint256 protocolFee, uint160 sqrtPriceX96, int24 tick, address owner);
    }
    #[sol(rpc)]
    interface ProtectedPerp {
        function withdrawalController() external view returns (address);
        function owner() external view returns (address);
        function governor() external view returns (address);
        function tvlCap() external view returns (uint256);
        function accountedTvl() external view returns (uint256);
        function lowerTvlCap(uint256 amount) external;
        event SetWithdrawalController(address indexed controller);
    }
    #[sol(rpc)]
    interface Controller {
        function perp() external view returns (address);
        function challengeWindow() external view returns (uint48);
        function releasesPaused() external view returns (bool);
        function rateLimit() external view returns (uint128 capacity, uint128 available, uint128 refillPerSecond, uint48 lastUpdated);
        function totalReserved() external view returns (uint256);
        function pauseReleases() external;
        function increaseChallengeWindow(uint48 newWindow) external;
        function unpauseReleases() external;
    }
    #[sol(rpc)]
    interface Governor {
        function getMinDelay() external view returns (uint256);
        function nonce() external view returns (uint256);
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ControllerTarget {
    pub perp: String,
    /// Receipt proving creation by a configured factory.
    pub deployment_transaction: String,
    pub controller: Option<String>,
    /// Required for a historical controller: a receipt with the Perp's attachment event.
    pub attachment_transaction: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatusRequest {
    pub targets: Vec<ControllerTarget>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ControllerStatus {
    pub chain_id: u64,
    pub block_number: u64,
    pub timestamp: u64,
    pub perp: String,
    pub controller: String,
    pub current_controller: String,
    pub guardian: String,
    pub governor: String,
    pub governor_delay_seconds: Option<String>,
    pub releases_paused: bool,
    pub challenge_window: u64,
    pub capacity: String,
    pub available: String,
    pub refill_per_second: String,
    pub total_reserved: String,
    pub accounted_tvl: String,
    pub tvl_cap: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControllerAction {
    Pause {},
    ExtendWindow {
        seconds: u64,
    },
    /// Output is addressed to the governor, which may need a separate timelock scheduling flow.
    Reopen {},
    LowerTvlCap {
        amount: String,
    },
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrepareRequest {
    pub targets: Vec<ControllerTarget>,
    #[serde(rename = "operation")]
    pub action: ControllerAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BuilderTransaction {
    pub to: String,
    pub value: String,
    pub data: String,
}
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BuilderFile {
    pub version: String,
    pub chain_id: String,
    pub created_at: u64,
    pub meta: BuilderMeta,
    pub transactions: Vec<BuilderTransaction>,
}
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BuilderMeta {
    pub name: String,
    pub description: String,
    pub tx_builder_version: String,
    pub created_from_safe_address: String,
}
#[derive(Debug, Serialize, JsonSchema)]
pub struct PreparedGroup {
    pub authority: String,
    pub safe_nonce: Option<String>,
    pub requires_governor_execution: bool,
    pub transaction_builder: BuilderFile,
}
#[derive(Debug, Serialize, JsonSchema)]
pub struct PreparedActions {
    pub status: String,
    pub states: Vec<ControllerStatus>,
    pub groups: Vec<PreparedGroup>,
}

fn address(value: &str) -> Result<Address, Status> {
    let parsed: Address = value.parse().map_err(|_| Status::BadRequest)?;
    if parsed.is_zero() {
        return Err(Status::BadRequest);
    }
    Ok(parsed)
}

pub fn available_capacity(
    capacity: u128,
    available: u128,
    refill: u128,
    updated: u64,
    now: u64,
) -> u128 {
    available
        .saturating_add(refill.saturating_mul(u128::from(now.saturating_sub(updated))))
        .min(capacity)
}

async fn read_targets(
    state: &AppState,
    targets: &[ControllerTarget],
) -> Result<Vec<ControllerStatus>, Status> {
    if targets.is_empty() || targets.len() > 64 {
        return Err(Status::BadRequest);
    }
    let provider = &state.provider.read_provider;
    let block = provider
        .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
        .await
        .map_err(|_| Status::BadGateway)?
        .ok_or(Status::BadGateway)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| Status::ServiceUnavailable)?
        .as_secs();
    if block.header.timestamp > now + 30 || now.saturating_sub(block.header.timestamp) > 120 {
        return Err(Status::ServiceUnavailable);
    }
    let at = BlockId::from(block.header.number);
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for target in targets {
        let perp = address(&target.perp)?;
        let hash: B256 = target
            .deployment_transaction
            .parse()
            .map_err(|_| Status::BadRequest)?;
        let receipt = provider
            .get_transaction_receipt(hash)
            .await
            .map_err(|_| Status::BadGateway)?
            .ok_or(Status::Conflict)?;
        if !receipt.status() || receipt.block_number.is_none_or(|n| n > block.header.number) {
            return Err(Status::Conflict);
        }
        let valid_creation = receipt.logs().iter().any(|entry| {
            (entry.address() == state.contracts.perp_factory
                || state
                    .contracts
                    .legacy_perp_factories
                    .contains(&entry.address()))
                && entry
                    .log_decode::<ProtectedFactory::PerpCreated>()
                    .is_ok_and(|decoded| decoded.inner.data.perp == perp)
        });
        if !valid_creation {
            return Err(Status::BadRequest);
        }
        let market = ProtectedPerp::new(perp, provider);
        let current = market
            .withdrawalController()
            .call()
            .block(at)
            .await
            .map_err(|_| Status::BadGateway)?;
        let controller = match &target.controller {
            Some(c) => address(c)?,
            None => current,
        };
        if controller.is_zero() {
            return Err(Status::Conflict);
        }
        if controller != current {
            let hash = target
                .attachment_transaction
                .as_ref()
                .ok_or(Status::BadRequest)?
                .parse::<B256>()
                .map_err(|_| Status::BadRequest)?;
            let receipt = provider
                .get_transaction_receipt(hash)
                .await
                .map_err(|_| Status::BadGateway)?
                .ok_or(Status::Conflict)?;
            if !receipt.status()
                || receipt.block_number.is_none_or(|n| n > block.header.number)
                || !receipt.logs().iter().any(|entry| {
                    entry.address() == perp
                        && entry
                            .log_decode::<ProtectedPerp::SetWithdrawalController>()
                            .is_ok_and(|decoded| decoded.inner.data.controller == controller)
                })
            {
                return Err(Status::BadRequest);
            }
        }
        if !seen.insert(controller) {
            return Err(Status::BadRequest);
        }
        let contract = Controller::new(controller, provider);
        if contract
            .perp()
            .call()
            .block(at)
            .await
            .map_err(|_| Status::BadGateway)?
            != perp
        {
            return Err(Status::Conflict);
        }
        let guardian = market
            .owner()
            .call()
            .block(at)
            .await
            .map_err(|_| Status::BadGateway)?;
        let governor = market
            .governor()
            .call()
            .block(at)
            .await
            .map_err(|_| Status::BadGateway)?;
        let bucket = contract
            .rateLimit()
            .call()
            .block(at)
            .await
            .map_err(|_| Status::BadGateway)?;
        let governor_delay = Governor::new(governor, provider)
            .getMinDelay()
            .call()
            .block(at)
            .await
            .ok();
        out.push(ControllerStatus {
            chain_id: state.provider.chain_id,
            block_number: block.header.number,
            timestamp: block.header.timestamp,
            perp: perp.to_string(),
            controller: controller.to_string(),
            current_controller: current.to_string(),
            guardian: guardian.to_string(),
            governor: governor.to_string(),
            governor_delay_seconds: governor_delay.map(|d| d.to_string()),
            releases_paused: contract
                .releasesPaused()
                .call()
                .block(at)
                .await
                .map_err(|_| Status::BadGateway)?,
            challenge_window: contract
                .challengeWindow()
                .call()
                .block(at)
                .await
                .map_err(|_| Status::BadGateway)?
                .to::<u64>(),
            capacity: bucket.capacity.to_string(),
            available: available_capacity(
                bucket.capacity,
                bucket.available,
                bucket.refillPerSecond,
                bucket.lastUpdated.to::<u64>(),
                block.header.timestamp,
            )
            .to_string(),
            refill_per_second: bucket.refillPerSecond.to_string(),
            total_reserved: contract
                .totalReserved()
                .call()
                .block(at)
                .await
                .map_err(|_| Status::BadGateway)?
                .to_string(),
            accounted_tvl: market
                .accountedTvl()
                .call()
                .block(at)
                .await
                .map_err(|_| Status::BadGateway)?
                .to_string(),
            tvl_cap: market
                .tvlCap()
                .call()
                .block(at)
                .await
                .map_err(|_| Status::BadGateway)?
                .to_string(),
        });
    }
    Ok(out)
}

#[openapi(tag = "Withdrawal protection")]
#[post("/withdrawals/status", data = "<request>")]
pub async fn withdrawal_status(
    request: Json<StatusRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<Vec<ControllerStatus>>, Status> {
    Ok(Json(read_targets(state, &request.targets).await?))
}

pub fn action_call(
    status: &ControllerStatus,
    action: &ControllerAction,
) -> Result<Option<(Address, BuilderTransaction)>, Status> {
    let (authority, target, data) = match action {
        ControllerAction::Pause {} if status.releases_paused => return Ok(None),
        ControllerAction::Pause {} => (
            address(&status.guardian)?,
            address(&status.controller)?,
            Controller::pauseReleasesCall {}.abi_encode(),
        ),
        ControllerAction::ExtendWindow { seconds } => {
            if *seconds <= status.challenge_window || *seconds >= (1_u64 << 48) {
                return Err(Status::BadRequest);
            }
            (
                address(&status.guardian)?,
                address(&status.controller)?,
                Controller::increaseChallengeWindowCall {
                    newWindow: alloy::primitives::aliases::U48::from(*seconds),
                }
                .abi_encode(),
            )
        }
        ControllerAction::Reopen {} if !status.releases_paused => return Ok(None),
        ControllerAction::Reopen {} => (
            address(&status.governor)?,
            address(&status.controller)?,
            Controller::unpauseReleasesCall {}.abi_encode(),
        ),
        ControllerAction::LowerTvlCap { amount } => {
            let amount: U256 = amount.parse().map_err(|_| Status::BadRequest)?;
            let current: U256 = status.tvl_cap.parse().map_err(|_| Status::BadGateway)?;
            if amount >= current {
                return Err(Status::BadRequest);
            }
            (
                address(&status.guardian)?,
                address(&status.perp)?,
                ProtectedPerp::lowerTvlCapCall { amount }.abi_encode(),
            )
        }
    };
    Ok(Some((
        authority,
        BuilderTransaction {
            to: target.to_string(),
            value: "0".into(),
            data: format!("0x{}", hex::encode(data)),
        },
    )))
}

#[openapi(tag = "Withdrawal protection")]
#[post("/withdrawals/prepare", data = "<request>")]
pub async fn prepare_withdrawals(
    request: Json<PrepareRequest>,
    _admin: AdminToken,
    state: &State<AppState>,
) -> Result<Json<PreparedActions>, Status> {
    let mut targets = request.targets.clone();
    let initial = read_targets(state, &targets).await?;
    // A saved registry can predate a rotation. Include the live current
    // controller as well as all explicitly selected historical escrow.
    for snapshot in &initial {
        if snapshot.current_controller == Address::ZERO.to_string()
            || initial
                .iter()
                .any(|s| s.controller == snapshot.current_controller)
        {
            continue;
        }
        if let Some(proof) = request
            .targets
            .iter()
            .find(|t| t.perp.eq_ignore_ascii_case(&snapshot.perp))
        {
            let mut current = proof.clone();
            current.controller = None;
            current.attachment_transaction = None;
            if !targets
                .iter()
                .any(|t| t.perp.eq_ignore_ascii_case(&current.perp) && t.controller.is_none())
            {
                targets.push(current);
            }
        }
    }
    // Read the final set again at one block; never return a partial fleet batch.
    let states = read_targets(state, &targets).await?;
    let mut grouped: BTreeMap<Address, Vec<BuilderTransaction>> = BTreeMap::new();
    for snapshot in &states {
        if let Some((authority, call)) = action_call(snapshot, &request.action)? {
            let data: alloy::primitives::Bytes =
                call.data.parse().map_err(|_| Status::InternalServerError)?;
            state
                .provider
                .read_provider
                .call(
                    alloy::rpc::types::TransactionRequest::default()
                        .from(authority)
                        .to(address(&call.to)?)
                        .input(data.into()),
                )
                .block(BlockId::from(snapshot.block_number))
                .await
                .map_err(|_| Status::Conflict)?;
            let group = grouped.entry(authority).or_default();
            if !group
                .iter()
                .any(|existing| existing.to == call.to && existing.data == call.data)
            {
                group.push(call);
            }
        }
    }
    let mut groups = Vec::new();
    for (authority, transactions) in grouped {
        let nonce = Governor::new(authority, &state.provider.read_provider)
            .nonce()
            .call()
            .block(BlockId::from(states[0].block_number))
            .await
            .ok();
        groups.push(PreparedGroup {
            authority: authority.to_string(), safe_nonce: nonce.map(|n| n.to_string()),
            requires_governor_execution: matches!(request.action, ControllerAction::Reopen {}),
            transaction_builder: BuilderFile {
                version: "1.0".into(), chain_id: state.provider.chain_id.to_string(), created_at: states[0].timestamp * 1000,
                meta: BuilderMeta {
                    name: "Withdrawal protection".into(),
                    description: "Prepared only. Verify targets, executable Safe nonce and pending conflicts before signing; governor calls may require timelock scheduling. Verify chain state after execution.".into(),
                    tx_builder_version: "1.16.5".into(), created_from_safe_address: authority.to_string(),
                }, transactions,
            },
        });
    }
    Ok(Json(PreparedActions {
        status: "prepared".into(),
        states,
        groups,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> ControllerStatus {
        ControllerStatus {
            chain_id: 42161,
            block_number: 1,
            timestamp: 1,
            perp: Address::repeat_byte(1).to_string(),
            controller: Address::repeat_byte(2).to_string(),
            current_controller: Address::repeat_byte(2).to_string(),
            guardian: Address::repeat_byte(3).to_string(),
            governor: Address::repeat_byte(4).to_string(),
            governor_delay_seconds: Some("172800".into()),
            releases_paused: false,
            challenge_window: 172800,
            capacity: "100".into(),
            available: "100".into(),
            refill_per_second: "1".into(),
            total_reserved: "0".into(),
            accounted_tvl: "10".into(),
            tvl_cap: "1000".into(),
        }
    }
    #[test]
    fn unit_tests_pause_uses_guardian_and_reopen_uses_governor() {
        let mut s = snapshot();
        let (role, call) = action_call(&s, &ControllerAction::Pause {})
            .unwrap()
            .unwrap();
        assert_eq!(role.to_string(), s.guardian);
        assert_eq!(call.to, s.controller);
        assert_eq!(
            call.data,
            format!("0x{}", hex::encode(Controller::pauseReleasesCall::SELECTOR))
        );
        s.releases_paused = true;
        assert!(
            action_call(&s, &ControllerAction::Pause {})
                .unwrap()
                .is_none()
        );
        assert_eq!(
            action_call(&s, &ControllerAction::Reopen {})
                .unwrap()
                .unwrap()
                .0
                .to_string(),
            s.governor
        );
    }
    #[test]
    fn unit_tests_no_loosening_through_restrict_actions() {
        let s = snapshot();
        assert!(action_call(&s, &ControllerAction::ExtendWindow { seconds: 1 }).is_err());
        assert!(action_call(&s, &ControllerAction::ExtendWindow { seconds: 1 << 48 }).is_err());
        assert!(
            action_call(
                &s,
                &ControllerAction::LowerTvlCap {
                    amount: "1001".into()
                }
            )
            .is_err()
        );
        assert!(
            action_call(
                &s,
                &ControllerAction::LowerTvlCap {
                    amount: "500".into()
                }
            )
            .is_ok()
        );
    }
    #[test]
    fn unit_tests_prepare_request_rejects_unknown_actions_and_fields() {
        let target = serde_json::json!({"perp": "x", "deployment_transaction": "y", "controller": null, "attachment_transaction": null});
        let valid =
            serde_json::json!({"targets": [target.clone()], "operation": {"action":"pause"}});
        assert!(serde_json::from_value::<PrepareRequest>(valid).is_ok());
        let invalid = serde_json::json!({"targets": [target.clone()], "operation": {"action":"reduce_claim"}});
        assert!(serde_json::from_value::<PrepareRequest>(invalid).is_err());
        let invalid =
            serde_json::json!({"targets": [target], "operation": {"action":"pause", "sign":true}});
        assert!(serde_json::from_value::<PrepareRequest>(invalid).is_err());
    }
    #[test]
    fn unit_tests_capacity_saturates_without_overflow() {
        assert_eq!(available_capacity(100, 10, u128::MAX, 0, u64::MAX), 100);
        assert_eq!(available_capacity(0, 0, 0, 0, u64::MAX), 0);
        assert_eq!(available_capacity(100, 10, 2, 10, 20), 30);
    }
}

/// Explicit profile for the new factory generation; never default production risk limits.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProtectedDeploymentRequest {
    pub beacon: String,
    pub owner: String,
    pub governor: String,
    pub ema_window: u32,
    pub latency: u32,
    pub salt: String,
    pub capacity: String,
    pub refill_per_second: String,
    pub challenge_window: u64,
    pub tvl_cap: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ProtectedDeployment {
    pub status: String,
    pub desired_tvl_cap: String,
    pub governor_delay_seconds: String,
    pub transaction_builder: BuilderFile,
    pub next_step: String,
}

pub fn deployment_calldata(
    request: &ProtectedDeploymentRequest,
    modules: ProtectedFactory::Modules,
) -> Result<Vec<u8>, Status> {
    let owner = address(&request.owner)?;
    let governor = address(&request.governor)?;
    if request.ema_window == 0
        || request.ema_window > 0xffffff
        || request.challenge_window == 0
        || request.challenge_window >= (1_u64 << 48)
    {
        return Err(Status::BadRequest);
    }
    let cap: U256 = request.tvl_cap.parse().map_err(|_| Status::BadRequest)?;
    if cap == U256::MAX {
        return Err(Status::BadRequest);
    }
    let capacity = request
        .capacity
        .parse::<u128>()
        .map_err(|_| Status::BadRequest)?;
    let refill = request
        .refill_per_second
        .parse::<u128>()
        .map_err(|_| Status::BadRequest)?;
    let salt: B256 = request.salt.parse().map_err(|_| Status::BadRequest)?;
    Ok(ProtectedFactory::createPerpCall {
        owner,
        governor,
        modules,
        emaWindow: alloy::primitives::aliases::U24::from(request.ema_window),
        latency: request.latency,
        salt,
        withdrawalConfig: ProtectedFactory::WithdrawalConfig {
            capacity,
            refillPerSecond: refill,
            challengeWindow: alloy::primitives::aliases::U48::from(request.challenge_window),
        },
    }
    .abi_encode())
}

/// Prepares deployment through the owner Safe; retries never broadcast another creation.
#[openapi(tag = "Withdrawal protection")]
#[post("/withdrawals/deployment", data = "<request>")]
pub async fn prepare_protected_deployment(
    request: Json<ProtectedDeploymentRequest>,
    _admin: AdminToken,
    state: &State<AppState>,
) -> Result<Json<ProtectedDeployment>, Status> {
    let owner = address(&request.owner)?;
    let governor = address(&request.governor)?;
    let safe = state
        .contracts
        .safe
        .as_ref()
        .ok_or(Status::ServiceUnavailable)?;
    if owner != safe.address {
        return Err(Status::Conflict);
    }
    let provider = &state.provider.read_provider;
    let block = provider
        .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
        .await
        .map_err(|_| Status::BadGateway)?
        .ok_or(Status::BadGateway)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| Status::ServiceUnavailable)?
        .as_secs();
    if block.header.timestamp > now + 30 || now.saturating_sub(block.header.timestamp) > 120 {
        return Err(Status::ServiceUnavailable);
    }
    let at = BlockId::from(block.header.number);
    let delay = Governor::new(governor, provider)
        .getMinDelay()
        .call()
        .block(at)
        .await
        .map_err(|_| Status::Conflict)?;
    if delay.is_zero() {
        return Err(Status::Conflict);
    }
    let modules = ProtectedFactory::Modules {
        beacon: address(&request.beacon)?,
        fees: state.contracts.fees_module,
        funding: state.contracts.funding_module,
        marginRatios: state.contracts.margin_ratios_module,
        priceImpact: state.contracts.price_impact_module,
        pricing: state.contracts.pricing_module,
    };
    let data = deployment_calldata(&request, modules)?;
    provider
        .call(
            alloy::rpc::types::TransactionRequest::default()
                .from(owner)
                .to(state.contracts.perp_factory)
                .input(alloy::primitives::Bytes::from(data.clone()).into()),
        )
        .block(at)
        .await
        .map_err(|_| Status::Conflict)?;
    Ok(Json(ProtectedDeployment {
        status: "prepared".into(), desired_tvl_cap: request.tvl_cap.clone(), governor_delay_seconds: delay.to_string(),
        transaction_builder: BuilderFile {
            version: "1.0".into(), chain_id: state.provider.chain_id.to_string(), created_at: block.header.timestamp * 1000,
            meta: BuilderMeta { name: "Protected market deployment".into(), description: "Execute once, record the PerpCreated receipt, then configure the finite TVL cap and verify protection before listing.".into(), tx_builder_version: "1.16.5".into(), created_from_safe_address: owner.to_string() },
            transactions: vec![BuilderTransaction { to: state.contracts.perp_factory.to_string(), value: "0".into(), data: format!("0x{}", hex::encode(data)) }],
        },
        next_step: "Record deployment receipt; prepare lower_tvl_cap with the desired cap, execute through the owner Safe, then verify controller configuration before listing. A prepared deployment is not ready.".into(),
    }))
}
