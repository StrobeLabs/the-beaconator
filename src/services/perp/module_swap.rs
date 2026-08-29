//! Batch module swaps on per-market Perp contracts via the owning Gnosis Safe.
//!
//! Every deployed Perp (perpcity-contracts@v0.1.0) exposes six timelocked module
//! setters gated by a two-step: the owner calls `submit(data)`, then after
//! `timelock` seconds anyone may call the setter with calldata byte-identical to
//! the submitted `data`. Every perp is owned by a Gnosis Safe, so both steps are
//! MultiSendCallOnly batches routed through `Safe.execTransaction`:
//!
//! - Testnet: the Safe is 1-of-1 and its sole owner is the beaconator signer.
//!   The signer signs the Safe tx hash (EIP-712) and a KMS pool wallet
//!   broadcasts `execTransaction` — the signer itself never sends transactions.
//! - Mainnet: the Safe is a 2-of-N multisig. The batches are proposed to the
//!   Safe Transaction Service and humans co-sign + execute them in the Safe UI.
//!
//! The path is auto-detected from `getOwners()` / `getThreshold()`.

use alloy::primitives::{Address, Bytes, U256, address};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use alloy::signers::Signer;
use alloy::sol_types::SolCall;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::timeout;
use tracing;

use crate::models::{AppState, PendingTimelock, SwapModuleResponse};
use crate::routes::{IGnosisSafe, IModuleProbes, IMultiSendCallOnly, IPerpAdmin};
use crate::services::safe::SafeTransactionService;

/// Canonical Safe MultiSendCallOnly v1.3.0 — the same address on Arbitrum One
/// and Arbitrum Sepolia (deterministic deployment).
pub const MULTI_SEND_CALL_ONLY: Address = address!("9641d764fc13c8B624c04430C7356C1C7C8102e2");

/// Upper bound on perps per request. Bounds calldata size and blast radius, not
/// gas: the 52-perp submit batch executed on 2026-07-23 used ~2.0M gas.
pub const MAX_PERPS_PER_REQUEST: usize = 100;

const RECEIPT_TIMEOUT: Duration = Duration::from_secs(90);

/// Safe transaction operation values.
const OPERATION_CALL: u8 = 0;
const OPERATION_DELEGATECALL: u8 = 1;

/// The module slot to swap. Mirrors the `Modules` struct in
/// perpcity-contracts@v0.1.0 `SharedStructs.sol`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleType {
    Beacon,
    Pricing,
    Funding,
    Fees,
    MarginRatios,
    PriceImpact,
}

impl ModuleType {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "beacon" => Some(Self::Beacon),
            "pricing" => Some(Self::Pricing),
            "funding" => Some(Self::Funding),
            "fees" => Some(Self::Fees),
            "margin_ratios" => Some(Self::MarginRatios),
            "price_impact" => Some(Self::PriceImpact),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Beacon => "beacon",
            Self::Pricing => "pricing",
            Self::Funding => "funding",
            Self::Fees => "fees",
            Self::MarginRatios => "margin_ratios",
            Self::PriceImpact => "price_impact",
        }
    }

    /// Calldata of the perp's setter for this slot — the bytes that must first be
    /// `submit`ted and then re-sent verbatim as the setter call.
    pub fn setter_calldata(&self, new_module: Address) -> Vec<u8> {
        match self {
            Self::Beacon => IPerpAdmin::setBeaconCall {
                newBeacon: new_module,
            }
            .abi_encode(),
            Self::Pricing => IPerpAdmin::setPricingModuleCall {
                newPricing: new_module,
            }
            .abi_encode(),
            Self::Funding => IPerpAdmin::setFundingModuleCall {
                newFunding: new_module,
            }
            .abi_encode(),
            Self::Fees => IPerpAdmin::setFeesModuleCall {
                newFees: new_module,
            }
            .abi_encode(),
            Self::MarginRatios => IPerpAdmin::setMarginRatiosModuleCall {
                newMarginRatios: new_module,
            }
            .abi_encode(),
            Self::PriceImpact => IPerpAdmin::setPriceImpactModuleCall {
                newPriceImpact: new_module,
            }
            .abi_encode(),
        }
    }
}

/// Which half of the timelocked two-step to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapPhase {
    Both,
    Submit,
    Execute,
}

impl SwapPhase {
    pub fn parse(s: Option<&str>) -> Option<Self> {
        match s {
            None | Some("both") => Some(Self::Both),
            Some("submit") => Some(Self::Submit),
            Some("execute") => Some(Self::Execute),
            _ => None,
        }
    }
}

/// Errors split by who is at fault: `Validation` maps to a caller-visible
/// failure message (bad input / preconditions), `Internal` to a 500.
#[derive(Debug)]
pub enum SwapModuleError {
    Validation(String),
    Internal(String),
}

impl SwapModuleError {
    pub fn message(&self) -> &str {
        match self {
            Self::Validation(m) | Self::Internal(m) => m,
        }
    }
}

/// One MultiSendCallOnly item: `operation (1) ++ to (20) ++ value (32) ++
/// dataLength (32) ++ data`.
pub fn multisend_item(to: Address, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(85 + data.len());
    out.push(OPERATION_CALL);
    out.extend_from_slice(to.as_slice());
    out.extend_from_slice(&U256::ZERO.to_be_bytes::<32>());
    out.extend_from_slice(&U256::from(data.len()).to_be_bytes::<32>());
    out.extend_from_slice(data);
    out
}

/// Concatenated MultiSend items calling `data` on every perp.
pub fn build_batch(perps: &[Address], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(perps.len() * (85 + data.len()));
    for perp in perps {
        out.extend_from_slice(&multisend_item(*perp, data));
    }
    out
}

/// `submit(bytes)` calldata wrapping the setter calldata.
pub fn submit_calldata(setter: &[u8]) -> Vec<u8> {
    IPerpAdmin::submitCall {
        data: Bytes::copy_from_slice(setter),
    }
    .abi_encode()
}

/// Full `multiSend(bytes)` calldata for a batch — the `data` argument of
/// `Safe.execTransaction` (and the bytes the Safe tx hash commits to).
pub fn multisend_calldata(batch: &[u8]) -> Vec<u8> {
    IMultiSendCallOnly::multiSendCall {
        transactions: Bytes::copy_from_slice(batch),
    }
    .abi_encode()
}

struct PerpChecks {
    timelock: u64,
    executable_at: u64,
}

/// Swap `module_type` to `new_module` on every perp in `perps`.
pub async fn swap_module(
    state: &AppState,
    module_type: ModuleType,
    new_module: Address,
    perps: Vec<Address>,
    phase: SwapPhase,
) -> Result<SwapModuleResponse, SwapModuleError> {
    let safe_config = state.contracts.safe.as_ref().ok_or_else(|| {
        SwapModuleError::Validation(
            "SAFE_ADDRESS is not configured; module swaps go through the Safe that owns the perps"
                .to_string(),
        )
    })?;
    let safe_address = safe_config.address;

    if perps.is_empty() || perps.len() > MAX_PERPS_PER_REQUEST {
        return Err(SwapModuleError::Validation(format!(
            "perp_addresses must contain 1..={MAX_PERPS_PER_REQUEST} entries, got {}",
            perps.len()
        )));
    }

    let provider = &state.provider.read_provider;

    // New module must be a deployed contract.
    let code = provider
        .get_code_at(new_module)
        .await
        .map_err(|e| SwapModuleError::Internal(format!("Failed to read module code: {e}")))?;
    if code.is_empty() {
        return Err(SwapModuleError::Validation(format!(
            "new_module_address {new_module} has no deployed code"
        )));
    }

    // Interface probe: the type-specific view must not revert with zeroed args.
    probe_module(state, module_type, new_module).await?;

    let setter = module_type.setter_calldata(new_module);

    // Per-perp preconditions. Sequential reads: bounded by MAX_PERPS_PER_REQUEST
    // and this is an operator route, not a hot path.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| SwapModuleError::Internal(format!("System clock error: {e}")))?
        .as_secs();
    let mut targets: Vec<Address> = Vec::new();
    let mut checks: Vec<PerpChecks> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    for perp in &perps {
        match check_perp(state, *perp, safe_address, module_type, new_module, &setter).await {
            Ok(None) => skipped.push(perp.to_checksum(None)),
            Ok(Some(c)) => {
                if phase == SwapPhase::Execute {
                    if c.executable_at == 0 {
                        failures.push(format!(
                            "{}: no pending submit for this swap (run phase=submit first)",
                            perp.to_checksum(None)
                        ));
                        continue;
                    }
                    if c.executable_at > now {
                        failures.push(format!(
                            "{}: timelock not expired (executable at {})",
                            perp.to_checksum(None),
                            c.executable_at
                        ));
                        continue;
                    }
                }
                targets.push(*perp);
                checks.push(c);
            }
            Err(msg) => failures.push(msg),
        }
    }
    if !failures.is_empty() {
        return Err(SwapModuleError::Validation(format!(
            "validation failed for {} perp(s): {}",
            failures.len(),
            failures.join("; ")
        )));
    }

    if targets.is_empty() {
        return Ok(SwapModuleResponse {
            mode: "executed".to_string(),
            submit_tx_hash: None,
            execute_tx_hash: None,
            swapped: vec![],
            skipped_already_set: skipped,
            pending_timelock: vec![],
        });
    }

    let any_timelocked = checks.iter().any(|c| c.timelock > 0);

    // Path detection: direct execution when the beaconator signer alone controls
    // the Safe, proposal to the Safe Transaction Service otherwise.
    let safe_contract = IGnosisSafe::new(safe_address, provider.as_ref());
    let owners = safe_contract
        .getOwners()
        .call()
        .await
        .map_err(|e| SwapModuleError::Internal(format!("Failed to read Safe owners: {e}")))?;
    let threshold =
        safe_contract.getThreshold().call().await.map_err(|e| {
            SwapModuleError::Internal(format!("Failed to read Safe threshold: {e}"))
        })?;
    let direct = threshold == U256::from(1) && owners.contains(&state.wallets.signer_address);

    let submit_batch = build_batch(&targets, &submit_calldata(&setter));
    let exec_batch = build_batch(&targets, &setter);

    tracing::info!(
        "swap_module: {} -> {} on {} perp(s) ({} skipped), phase {:?}, path {}",
        module_type.as_str(),
        new_module,
        targets.len(),
        skipped.len(),
        phase,
        if direct { "direct" } else { "propose" },
    );

    if direct {
        swap_direct(
            state,
            safe_address,
            module_type,
            phase,
            any_timelocked,
            &targets,
            &setter,
            submit_batch,
            exec_batch,
            skipped,
        )
        .await
    } else {
        swap_propose(
            state,
            safe_address,
            phase,
            &targets,
            &setter,
            submit_batch,
            exec_batch,
            skipped,
        )
        .await
    }
}

/// Staticcall the module-type-specific view with zeroed arguments.
async fn probe_module(
    state: &AppState,
    module_type: ModuleType,
    new_module: Address,
) -> Result<(), SwapModuleError> {
    let provider = state.provider.read_provider.as_ref();
    let probes = IModuleProbes::new(new_module, provider);
    let zero_pair = IModuleProbes::PricePair {
        ammPrice: 0,
        index: 0,
    };
    let result = match module_type {
        ModuleType::Beacon => probes.index().call().await.map(|_| ()),
        ModuleType::Pricing => probes
            .fairPrice(U256::ZERO, U256::ZERO, U256::ZERO, U256::ZERO)
            .call()
            .await
            .map(|_| ()),
        ModuleType::Funding => probes
            .funding(zero_pair.clone(), zero_pair)
            .call()
            .await
            .map(|_| ()),
        ModuleType::Fees => probes.fees().call().await.map(|_| ()),
        ModuleType::MarginRatios => probes.makerMarginRatios().call().await.map(|_| ()),
        ModuleType::PriceImpact => probes
            .sqrtPriceBounds(U256::ZERO, U256::ZERO, U256::ZERO, U256::ZERO)
            .call()
            .await
            .map(|_| ()),
    };
    result.map_err(|e| {
        SwapModuleError::Validation(format!(
            "new_module_address {new_module} failed the {} interface probe: {e}",
            module_type.as_str()
        ))
    })
}

/// Validate one perp. `Ok(None)` means the slot already holds the new module
/// (skip); `Ok(Some(checks))` carries its timelock state.
async fn check_perp(
    state: &AppState,
    perp: Address,
    safe_address: Address,
    module_type: ModuleType,
    new_module: Address,
    setter: &[u8],
) -> Result<Option<PerpChecks>, String> {
    let provider = state.provider.read_provider.as_ref();
    let perp_contract = IPerpAdmin::new(perp, provider);

    let owner = perp_contract
        .owner()
        .call()
        .await
        .map_err(|e| format!("{}: failed to read owner (is this a Perp?): {e}", perp))?;
    if owner != safe_address {
        return Err(format!(
            "{}: owned by {owner}, not the configured Safe {safe_address}",
            perp.to_checksum(None)
        ));
    }

    let modules = perp_contract
        .modules()
        .call()
        .await
        .map_err(|e| format!("{}: failed to read modules: {e}", perp))?;
    let current = match module_type {
        ModuleType::Beacon => modules.beacon,
        ModuleType::Fees => modules.fees,
        ModuleType::Funding => modules.funding,
        ModuleType::MarginRatios => modules.marginRatios,
        ModuleType::PriceImpact => modules.priceImpact,
        ModuleType::Pricing => modules.pricing,
    };
    if current == new_module {
        return Ok(None);
    }

    let timelock = perp_contract
        .timelock()
        .call()
        .await
        .map_err(|e| format!("{}: failed to read timelock: {e}", perp))?;
    let executable_at = perp_contract
        .executableAt(Bytes::copy_from_slice(setter))
        .call()
        .await
        .map_err(|e| format!("{}: failed to read executableAt: {e}", perp))?;

    Ok(Some(PerpChecks {
        timelock: timelock.saturating_to::<u64>(),
        executable_at: executable_at.saturating_to::<u64>(),
    }))
}

/// Sign the Safe tx hash with the beaconator signer and return the 65-byte
/// `r ++ s ++ v` signature Safe expects for a single ECDSA owner signature.
async fn sign_safe_tx(
    state: &AppState,
    safe_address: Address,
    ms_calldata: &[u8],
    nonce: u64,
) -> Result<Vec<u8>, SwapModuleError> {
    let hash = SafeTransactionService::encode_safe_tx_hash(
        safe_address,
        state.provider.chain_id,
        MULTI_SEND_CALL_ONLY,
        ms_calldata,
        OPERATION_DELEGATECALL,
        nonce,
    );
    let signature = state
        .wallets
        .signer
        .sign_hash(&hash)
        .await
        .map_err(|e| SwapModuleError::Internal(format!("Failed to sign Safe tx hash: {e}")))?;
    let mut sig = Vec::with_capacity(65);
    sig.extend_from_slice(&signature.r().to_be_bytes::<32>());
    sig.extend_from_slice(&signature.s().to_be_bytes::<32>());
    sig.push(if signature.v() { 28 } else { 27 });
    Ok(sig)
}

/// Read the on-chain `executableAt` for every target after a submit landed.
async fn read_pending_timelocks(
    state: &AppState,
    targets: &[Address],
    setter: &[u8],
) -> Vec<PendingTimelock> {
    let provider = state.provider.read_provider.as_ref();
    let mut out = Vec::with_capacity(targets.len());
    for perp in targets {
        let executable_at = IPerpAdmin::new(*perp, provider)
            .executableAt(Bytes::copy_from_slice(setter))
            .call()
            .await
            .map(|v| v.saturating_to::<u64>())
            .unwrap_or(0);
        out.push(PendingTimelock {
            perp_address: perp.to_checksum(None),
            executable_at,
        });
    }
    out
}

/// Direct path: signer signs, a pool wallet broadcasts `execTransaction`.
#[allow(clippy::too_many_arguments)]
async fn swap_direct(
    state: &AppState,
    safe_address: Address,
    module_type: ModuleType,
    phase: SwapPhase,
    any_timelocked: bool,
    targets: &[Address],
    setter: &[u8],
    submit_batch: Vec<u8>,
    exec_batch: Vec<u8>,
    skipped: Vec<String>,
) -> Result<SwapModuleResponse, SwapModuleError> {
    let wallet_handle = state
        .wallets
        .manager
        .acquire_any_wallet()
        .await
        .map_err(|e| SwapModuleError::Internal(format!("Failed to acquire wallet: {e}")))?;
    let wallet_provider = wallet_handle
        .build_provider(&state.provider.rpc_url)
        .map_err(|e| SwapModuleError::Internal(format!("Failed to build provider: {e}")))?;
    let safe_contract = IGnosisSafe::new(safe_address, &wallet_provider);

    let mut submit_tx_hash: Option<String> = None;
    let mut execute_tx_hash: Option<String> = None;

    if phase == SwapPhase::Both || phase == SwapPhase::Submit {
        let hash = send_exec_transaction(
            state,
            &wallet_handle,
            &safe_contract,
            safe_address,
            multisend_calldata(&submit_batch),
            "submit batch",
        )
        .await?;
        submit_tx_hash = Some(format!("{hash:#x}"));
    }

    let run_execute = match phase {
        SwapPhase::Submit => false,
        SwapPhase::Execute => true,
        SwapPhase::Both => !any_timelocked,
    };

    if run_execute {
        let hash = send_exec_transaction(
            state,
            &wallet_handle,
            &safe_contract,
            safe_address,
            multisend_calldata(&exec_batch),
            "execute batch",
        )
        .await?;
        execute_tx_hash = Some(format!("{hash:#x}"));
        tracing::info!(
            "swap_module: {} swapped on {} perp(s)",
            module_type.as_str(),
            targets.len()
        );
        return Ok(SwapModuleResponse {
            mode: "executed".to_string(),
            submit_tx_hash,
            execute_tx_hash,
            swapped: targets.iter().map(|p| p.to_checksum(None)).collect(),
            skipped_already_set: skipped,
            pending_timelock: vec![],
        });
    }

    // Submit landed but execution is deferred (explicit phase=submit or a
    // nonzero timelock): report on-chain executableAt for each target.
    let pending = read_pending_timelocks(state, targets, setter).await;
    Ok(SwapModuleResponse {
        mode: "submitted_only".to_string(),
        submit_tx_hash,
        execute_tx_hash,
        swapped: vec![],
        skipped_already_set: skipped,
        pending_timelock: pending,
    })
}

/// Sign at the Safe's current on-chain nonce, preflight with `eth_call`, send,
/// and wait for a successful receipt.
async fn send_exec_transaction<P: Provider>(
    state: &AppState,
    wallet_handle: &crate::services::wallet::WalletHandle,
    safe_contract: &IGnosisSafe::IGnosisSafeInstance<P>,
    safe_address: Address,
    ms_calldata: Vec<u8>,
    label: &str,
) -> Result<alloy::primitives::B256, SwapModuleError> {
    // Read the nonce fresh before each Safe tx: it advances on every successful
    // execTransaction, including the batch sent immediately before this one.
    let nonce = IGnosisSafe::new(safe_address, state.provider.read_provider.as_ref())
        .nonce()
        .call()
        .await
        .map_err(|e| SwapModuleError::Internal(format!("Failed to read Safe nonce: {e}")))?
        .saturating_to::<u64>();

    let sig = sign_safe_tx(state, safe_address, &ms_calldata, nonce).await?;
    let call = safe_contract.execTransaction(
        MULTI_SEND_CALL_ONLY,
        U256::ZERO,
        Bytes::from(ms_calldata),
        OPERATION_DELEGATECALL,
        U256::ZERO,
        U256::ZERO,
        U256::ZERO,
        Address::ZERO,
        Address::ZERO,
        Bytes::from(sig),
    );

    // Preflight: an eth_call surfaces the revert (with reason) before spending gas.
    // With all Safe gas params 0 an inner MultiSend failure reverts the whole tx,
    // so a passing preflight covers every perp in the batch.
    call.call().await.map_err(|e| {
        SwapModuleError::Validation(format!("{label} preflight simulation reverted: {e}"))
    })?;

    wallet_handle
        .ensure_lock_held()
        .map_err(SwapModuleError::Internal)?;
    let pending = call
        .send()
        .await
        .map_err(|e| SwapModuleError::Internal(format!("Failed to send {label}: {e}")))?;
    let tx_hash = *pending.tx_hash();
    tracing::info!("swap_module {label} sent: {tx_hash:#x}");

    let receipt = timeout(RECEIPT_TIMEOUT, pending.get_receipt())
        .await
        .map_err(|_| {
            SwapModuleError::Internal(format!(
                "Timed out waiting for {label} receipt ({tx_hash:#x})"
            ))
        })?
        .map_err(|e| {
            SwapModuleError::Internal(format!("Failed to get {label} receipt ({tx_hash:#x}): {e}"))
        })?;
    if !receipt.status() {
        return Err(SwapModuleError::Internal(format!(
            "{label} transaction reverted on-chain: {tx_hash:#x}"
        )));
    }
    Ok(tx_hash)
}

/// Proposal path: sign and queue the batches on the Safe Transaction Service
/// for the remaining owners to co-sign and execute in the Safe UI.
#[allow(clippy::too_many_arguments)]
async fn swap_propose(
    state: &AppState,
    safe_address: Address,
    phase: SwapPhase,
    targets: &[Address],
    setter: &[u8],
    submit_batch: Vec<u8>,
    exec_batch: Vec<u8>,
    skipped: Vec<String>,
) -> Result<SwapModuleResponse, SwapModuleError> {
    let safe_config = state.contracts.safe.as_ref().expect("checked by caller");
    let tx_service_url = safe_config.tx_service_url.as_ref().ok_or_else(|| {
        SwapModuleError::Validation(
            "Safe Transaction Service URL is not configured and the signer cannot execute \
             directly (multisig threshold > 1)"
                .to_string(),
        )
    })?;

    // Preflight the submit calls from the Safe before proposing: catches a wrong
    // perp list while the batches are still off-chain. The setter calls cannot be
    // simulated yet — they revert with DataNotTimelocked until the submit batch
    // executes.
    if phase == SwapPhase::Both || phase == SwapPhase::Submit {
        let submit_cd = submit_calldata(setter);
        for perp in targets {
            let tx_request = TransactionRequest::default()
                .from(safe_address)
                .to(*perp)
                .input(Bytes::from(submit_cd.clone()).into());
            state
                .provider
                .read_provider
                .estimate_gas(tx_request)
                .await
                .map_err(|e| {
                    SwapModuleError::Validation(format!(
                        "{}: submit would revert: {e}",
                        perp.to_checksum(None)
                    ))
                })?;
        }
    }

    let safe_service = SafeTransactionService::new(tx_service_url);
    let mut nonce = safe_service
        .get_nonce(safe_address)
        .await
        .map_err(SwapModuleError::Internal)?;

    let mut submit_tx_hash: Option<String> = None;
    let mut execute_tx_hash: Option<String> = None;

    if phase == SwapPhase::Both || phase == SwapPhase::Submit {
        let hash = safe_service
            .propose_transaction(
                safe_address,
                state.provider.chain_id,
                MULTI_SEND_CALL_ONLY,
                &multisend_calldata(&submit_batch),
                OPERATION_DELEGATECALL,
                nonce,
                &state.wallets.signer,
            )
            .await
            .map_err(SwapModuleError::Internal)?;
        submit_tx_hash = Some(format!("{hash:#x}"));
        nonce += 1;
    }

    if phase == SwapPhase::Both || phase == SwapPhase::Execute {
        let hash = safe_service
            .propose_transaction(
                safe_address,
                state.provider.chain_id,
                MULTI_SEND_CALL_ONLY,
                &multisend_calldata(&exec_batch),
                OPERATION_DELEGATECALL,
                nonce,
                &state.wallets.signer,
            )
            .await
            .map_err(SwapModuleError::Internal)?;
        execute_tx_hash = Some(format!("{hash:#x}"));
    }

    tracing::info!(
        "swap_module: proposed batches to Safe Transaction Service for {} perp(s)",
        targets.len()
    );
    Ok(SwapModuleResponse {
        mode: "proposed".to_string(),
        submit_tx_hash,
        execute_tx_hash,
        swapped: targets.iter().map(|p| p.to_checksum(None)).collect(),
        skipped_already_set: skipped,
        pending_timelock: vec![],
    })
}
