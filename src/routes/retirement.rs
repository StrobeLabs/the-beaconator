//! Retirement of the v0.1.0 perps this service deploys. Proposals never imply execution.
use crate::{
    guards::ApiToken,
    models::{ApiResponse, AppState},
    routes::IBeaconRegistry,
    services::{
        perp::perp_factory_of,
        safe::{SafeCall, SafeOperation, SafeTransactionService},
    },
};
use alloy::{
    primitives::{Address, B256, Bytes, U256, address},
    providers::Provider,
    rpc::types::state::StateOverridesBuilder,
    sol,
    sol_types::SolCall,
};
use rocket::{State, http::Status, post, serde::json::Json};
use rocket_okapi::openapi;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

sol! {
    #[sol(rpc)]
    interface RetirementPerp {
        function owner() external view returns (address);
        function modules() external view returns (address beacon, address fees, address funding, address marginRatios, address priceImpact, address pricing);
        function rates() external view returns (int88 fundingPerDay, uint64 longUtilFeePerDay, uint64 shortUtilFeePerDay, uint40 lastTouch);
        function executableAt(bytes data) external view returns (uint256);
        function abdicated(bytes4 selector) external view returns (bool);
        function timelock() external view returns (uint256);
        function submit(bytes data) external;
        function setFundingModule(address newFunding) external;
        function touch() external;
    }
    #[sol(rpc)]
    interface RetirementOwner {
        function owner() external view returns (address);
        function nonce() external view returns (uint256);
    }
    interface MultiSendCallOnly {
        function multiSend(bytes transactions) external payable;
    }
}

/// Canonical Safe v1.4.1 MultiSendCallOnly, the same address on Arbitrum One and Sepolia.
pub const MULTI_SEND_CALL_ONLY: Address = address!("0x9641d764fc13c8B624c04430C7356C1C7C8102e2");

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetirementIntent {
    Funding,
    Beacon,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetirementRequest {
    pub perp_address: String,
    pub intent: RetirementIntent,
    #[serde(default)]
    pub propose: bool,
    pub nonce: Option<u64>,
    pub expected_hash: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RetirementResponse {
    pub chain_id: u64,
    pub perp_address: String,
    pub beacon_address: String,
    pub owner: String,
    pub funding_stopped: bool,
    pub beacon_registered: bool,
    pub utilization_fees_active: bool,
    pub step: String,
    pub executable_at: String,
    pub safe_nonce: u64,
    pub nonce: Option<u64>,
    pub proposal_hash: Option<String>,
    pub safe_url: String,
    pub proposed: bool,
    /// One Safe transaction covers every remaining funding step.
    #[serde(default)]
    pub batched: bool,
}

fn zero_module(chain: u64) -> Result<Address, Status> {
    // Never accept a module from a browser. Additional deployments are operator configuration.
    if let Ok(value) = std::env::var("RETIREMENT_ZERO_FUNDING_MODULE") {
        return value.parse().map_err(|_| Status::ServiceUnavailable);
    }
    if chain == 42161 {
        return "0x8e562a533a92B47F9cF14300e42d6822a81aD4e1"
            .parse()
            .map_err(|_| Status::ServiceUnavailable);
    }
    Err(Status::ServiceUnavailable)
}

pub fn funding_step(installed: bool, rate_zero: bool, pending: U256, now: U256) -> &'static str {
    if installed {
        if rate_zero {
            "funding_stopped"
        } else {
            "refresh_rate"
        }
    } else if pending.is_zero() {
        "submit_funding"
    } else if pending > now {
        "timelock"
    } else {
        "set_funding"
    }
}

/// Pack calls for MultiSendCallOnly: operation, to, value, data length, data.
pub fn multi_send_data(calls: &[(Address, Vec<u8>)]) -> Vec<u8> {
    let mut packed = Vec::new();
    for (to, data) in calls {
        packed.push(SafeOperation::Call as u8);
        packed.extend_from_slice(to.as_slice());
        packed.extend_from_slice(&U256::ZERO.to_be_bytes::<32>());
        packed.extend_from_slice(&U256::from(data.len()).to_be_bytes::<32>());
        packed.extend_from_slice(data);
    }
    MultiSendCallOnly::multiSendCall {
        transactions: packed.into(),
    }
    .abi_encode()
}

/// Prepare an exact Safe proposal, or submit that same hash after the backend has persisted it.
#[openapi(tag = "Perpetual")]
#[post("/retire_perp", data = "<request>")]
pub async fn retire_perp(
    request: Json<RetirementRequest>,
    _token: ApiToken,
    state: &State<AppState>,
) -> Result<Json<ApiResponse<RetirementResponse>>, Status> {
    let perp: Address = request
        .perp_address
        .parse()
        .map_err(|_| Status::BadRequest)?;
    let zero = zero_module(state.provider.chain_id)?;
    if perp.is_zero() || zero.is_zero() {
        return Err(Status::BadRequest);
    }
    let provider = &state.provider.read_provider;
    let safe = state
        .contracts
        .safe
        .as_ref()
        .ok_or(Status::ServiceUnavailable)?;
    let service = SafeTransactionService::new(
        safe.tx_service_url
            .as_deref()
            .ok_or(Status::ServiceUnavailable)?,
    );
    let block = provider
        .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
        .await
        .map_err(|_| Status::BadGateway)?
        .ok_or(Status::BadGateway)?;
    let block_id = alloy::eips::BlockId::from(block.header.number);
    let safe_nonce: u64 = RetirementOwner::new(safe.address, provider)
        .nonce()
        .call()
        .block(block_id)
        .await
        .map_err(|_| Status::BadGateway)?
        .try_into()
        .map_err(|_| Status::Conflict)?;
    // The perp may sit on the primary factory or on a legacy one (old markets being wound
    // down after a contracts redeploy); both are trusted deployers.
    match perp_factory_of(&state.contracts, provider, perp, Some(block_id))
        .await
        .map_err(|_| Status::BadGateway)?
    {
        Some(factory) => {
            tracing::info!("retire_perp: {perp} registered with PerpFactory {factory}")
        }
        None => return Err(Status::BadRequest),
    }
    let contract = RetirementPerp::new(perp, provider);
    let owner = contract
        .owner()
        .call()
        .block(block_id)
        .await
        .map_err(|_| Status::BadGateway)?;
    if owner != safe.address {
        return Err(Status::Conflict);
    }
    let modules = contract
        .modules()
        .call()
        .block(block_id)
        .await
        .map_err(|_| Status::BadGateway)?;
    let rates = contract
        .rates()
        .call()
        .block(block_id)
        .await
        .map_err(|_| Status::BadGateway)?;
    if modules.funding != zero && modules.funding != state.contracts.funding_module {
        return Err(Status::Conflict);
    }
    // Exercise the deployed TWO-PricePair selector, never HEAD's incompatible three-argument ABI.
    let input = Bytes::from(
        hex::decode(format!(
            "87415bfc{:064x}{:064x}{:064x}{:064x}",
            1_u128 << 96,
            1_u128 << 96,
            2_u128 << 96,
            2_u128 << 96
        ))
        .map_err(|_| Status::InternalServerError)?,
    );
    let result = provider
        .call(
            alloy::rpc::types::TransactionRequest::default()
                .to(zero)
                .input(input.into()),
        )
        .block(block_id)
        .await
        .map_err(|_| Status::BadGateway)?;
    if result.as_ref() != [0_u8; 32] {
        return Err(Status::Conflict);
    }
    let registry = IBeaconRegistry::new(state.contracts.perpcity_registry, provider);
    let registered = registry
        .isBeaconRegistered(modules.beacon)
        .call()
        .block(block_id)
        .await
        .map_err(|_| Status::BadGateway)?;
    let setter = RetirementPerp::setFundingModuleCall { newFunding: zero }.abi_encode();
    let pending = contract
        .executableAt(setter.clone().into())
        .call()
        .block(block_id)
        .await
        .map_err(|_| Status::BadGateway)?;
    let stopped = modules.funding == zero && rates.fundingPerDay.is_zero();
    let step = match request.intent {
        RetirementIntent::Funding => funding_step(
            modules.funding == zero,
            rates.fundingPerDay.is_zero(),
            pending,
            U256::from(block.header.timestamp),
        ),
        RetirementIntent::Beacon if !stopped => return Err(Status::Conflict),
        RetirementIntent::Beacon if registered => "unregister_beacon",
        RetirementIntent::Beacon => "beacon_stopped",
    };
    // With no timelock, submit, apply and refresh are all valid in one Safe execution.
    // A perp that cannot report its timelock keeps the separate steps.
    let timelock = contract.timelock().call().block(block_id).await.ok();
    let multi_send = provider
        .get_code_at(MULTI_SEND_CALL_ONLY)
        .block_id(block_id)
        .await
        .map_err(|_| Status::BadGateway)?;
    let touch = RetirementPerp::touchCall {}.abi_encode();
    let calls: Vec<(Address, Vec<u8>)> = match step {
        "submit_funding" => {
            if contract
                .abdicated(RetirementPerp::setFundingModuleCall::SELECTOR.into())
                .call()
                .block(block_id)
                .await
                .map_err(|_| Status::BadGateway)?
            {
                return Err(Status::Conflict);
            }
            let submit = RetirementPerp::submitCall {
                data: setter.clone().into(),
            }
            .abi_encode();
            if timelock == Some(U256::ZERO) && !multi_send.is_empty() {
                vec![(perp, submit), (perp, setter), (perp, touch)]
            } else {
                vec![(perp, submit)]
            }
        }
        "set_funding" if !multi_send.is_empty() => vec![(perp, setter), (perp, touch)],
        "set_funding" => vec![(perp, setter)],
        "refresh_rate" => vec![(perp, touch)],
        "unregister_beacon" => {
            if RetirementOwner::new(state.contracts.perpcity_registry, provider)
                .owner()
                .call()
                .block(block_id)
                .await
                .map_err(|_| Status::BadGateway)?
                != safe.address
            {
                return Err(Status::Conflict);
            }
            vec![(
                state.contracts.perpcity_registry,
                IBeaconRegistry::unregisterBeaconCall {
                    beacon: modules.beacon,
                }
                .abi_encode(),
            )]
        }
        _ => Vec::new(),
    };
    let batched = calls.len() > 1;
    let (target, data, operation) = match calls.as_slice() {
        [(to, data)] => (*to, data.clone(), SafeOperation::Call),
        _ => (
            MULTI_SEND_CALL_ONLY,
            multi_send_data(&calls),
            SafeOperation::DelegateCall,
        ),
    };
    let safe_call = SafeCall {
        to: target,
        data: &data,
        operation,
    };
    let prefix = match state.provider.chain_id {
        42161 => "arb1",
        421614 => "arb-sep",
        _ => return Err(Status::ServiceUnavailable),
    };
    let mut response = RetirementResponse {
        chain_id: state.provider.chain_id,
        perp_address: perp.to_string(),
        beacon_address: modules.beacon.to_string(),
        owner: owner.to_string(),
        funding_stopped: stopped,
        beacon_registered: registered,
        utilization_fees_active: rates.longUtilFeePerDay != 0 || rates.shortUtilFeePerDay != 0,
        step: step.into(),
        executable_at: pending.to_string(),
        safe_nonce,
        nonce: None,
        proposal_hash: None,
        batched,
        safe_url: format!(
            "https://app.safe.global/transactions/queue?safe={prefix}:{}",
            safe.address
        ),
        proposed: false,
    };
    if calls.is_empty() {
        return Ok(retirement_response(response));
    }
    let nonce = match request.nonce {
        Some(n) => n,
        None => service
            .get_nonce(safe.address)
            .await
            .map_err(|_| Status::BadGateway)?,
    };
    if nonce < safe_nonce {
        return Err(Status::Conflict);
    }
    let hash = SafeTransactionService::encode_safe_tx_hash(
        safe.address,
        state.provider.chain_id,
        safe_call,
        nonce,
    );
    response.nonce = Some(nonce);
    response.proposal_hash = Some(format!("{hash:#x}"));
    if request.propose {
        if request
            .expected_hash
            .as_ref()
            .and_then(|s| s.parse::<B256>().ok())
            != Some(hash)
            || request.nonce.is_none()
        {
            return Err(Status::Conflict);
        }
        // Strict simulation before any proposal, including permissionless setters and touch.
        // A batch runs MultiSend's code in the Safe's context, so every call sees the Safe as sender.
        let request = alloy::rpc::types::TransactionRequest::default()
            .input(Bytes::from(data.clone()).into());
        match operation {
            SafeOperation::Call => provider
                .estimate_gas(request.from(safe.address).to(target))
                .await
                .map(|_| ()),
            SafeOperation::DelegateCall => provider
                .call(request.to(safe.address))
                .overrides(
                    StateOverridesBuilder::default()
                        .with_code(safe.address, multi_send.clone())
                        .build(),
                )
                .await
                .map(|_| ()),
        }
        .map_err(|_| Status::Conflict)?;
        if !service
            .proposal_exists(hash)
            .await
            .map_err(|_| Status::BadGateway)?
        {
            service
                .propose_transaction(
                    safe.address,
                    state.provider.chain_id,
                    safe_call,
                    nonce,
                    &state.wallets.signer,
                )
                .await
                .map_err(|_| Status::BadGateway)?;
        }
        response.proposed = true;
    }
    Ok(retirement_response(response))
}

fn retirement_response(response: RetirementResponse) -> Json<ApiResponse<RetirementResponse>> {
    Json(ApiResponse {
        success: true,
        message: if response.proposed {
            "Safe proposal submitted; execution remains pending"
        } else {
            "Retirement state verified"
        }
        .into(),
        data: Some(response),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unit_tests_multi_send_packs_each_call() {
        let to = Address::repeat_byte(0x11);
        let encoded = multi_send_data(&[(to, vec![0xaa, 0xbb]), (to, Vec::new())]);
        let decoded = MultiSendCallOnly::multiSendCall::abi_decode(&encoded).unwrap();
        let packed = decoded.transactions.to_vec();
        assert_eq!(packed.len(), 2 * 85 + 2);
        assert_eq!(packed[0], 0);
        assert_eq!(&packed[1..21], to.as_slice());
        assert_eq!(U256::from_be_slice(&packed[53..85]), U256::from(2));
        assert_eq!(&packed[85..87], &[0xaa, 0xbb]);
        assert_eq!(packed[87], 0);
    }

    #[test]
    fn unit_tests_retirement_requires_refresh_and_honors_timelock() {
        assert_eq!(
            funding_step(true, false, U256::ZERO, U256::from(100)),
            "refresh_rate"
        );
        assert_eq!(
            funding_step(true, true, U256::ZERO, U256::from(100)),
            "funding_stopped"
        );
        assert_eq!(
            funding_step(false, true, U256::ZERO, U256::from(100)),
            "submit_funding"
        );
        assert_eq!(
            funding_step(false, false, U256::from(101), U256::from(100)),
            "timelock"
        );
        assert_eq!(
            funding_step(false, false, U256::from(100), U256::from(100)),
            "set_funding"
        );
    }
}
