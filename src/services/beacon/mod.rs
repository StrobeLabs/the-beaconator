pub mod batch;
pub mod component_registry;
pub mod core;
pub mod ecdsa;
pub mod ecdsa_deploy;
pub mod factory;
pub mod modular;
pub mod recipe_registry;
pub mod registry;
pub mod verifiable;

pub use batch::*;
pub use component_registry::ComponentFactoryRegistry;
pub use core::*;
pub use ecdsa::*;
pub use ecdsa_deploy::create_ecdsa_verifier;
pub use factory::*;
pub use recipe_registry::RecipeRegistry;
pub use registry::BeaconTypeRegistry;
pub use verifiable::*;

/// Verify that a contract actually exists at `addr` (non-empty code).
///
/// Factory flows predict deployment addresses by simulating with `.call()` and
/// then trusting that address after `.send()`. If the prediction is wrong (state
/// changed between simulate and send, nonce drift, factory semantics), the flow
/// would register/return an address with no code behind it. A code-presence
/// check after the receipt catches that class of bug.
///
/// The check polls instead of reading once: load-balanced RPC providers (seen
/// consistently with Alchemy on Arbitrum Sepolia) can serve `eth_getCode` from
/// a replica that lags the node that confirmed the receipt, so an immediate
/// single read false-negatives on freshly deployed code. A genuinely wrong
/// prediction still fails once the poll budget is exhausted.
pub async fn verify_deployed(
    provider: &impl alloy::providers::Provider,
    addr: alloy::primitives::Address,
    label: &str,
) -> Result<(), String> {
    const ATTEMPTS: u32 = 12;
    const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

    let mut last_rpc_error: Option<String> = None;
    for attempt in 1..=ATTEMPTS {
        match provider.get_code_at(addr).await {
            Ok(code) if !code.is_empty() => {
                if attempt > 1 {
                    tracing::info!(
                        "{label} code at {addr} appeared on attempt {attempt} (RPC replica lag)"
                    );
                }
                return Ok(());
            }
            Ok(_) => last_rpc_error = None,
            Err(e) => last_rpc_error = Some(e.to_string()),
        }
        if attempt < ATTEMPTS {
            tokio::time::sleep(RETRY_DELAY).await;
        }
    }
    match last_rpc_error {
        Some(e) => Err(format!(
            "Failed to verify {label} deployment at {addr}: {e}"
        )),
        None => Err(format!(
            "{label} at {addr} has no deployed code after confirmation — predicted CREATE \
             address may be wrong; refusing to use it"
        )),
    }
}
