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
pub async fn verify_deployed(
    provider: &impl alloy::providers::Provider,
    addr: alloy::primitives::Address,
    label: &str,
) -> Result<(), String> {
    match provider.get_code_at(addr).await {
        Ok(code) if code.is_empty() => Err(format!(
            "{label} at {addr} has no deployed code after confirmation — predicted CREATE \
             address may be wrong; refusing to use it"
        )),
        Ok(_) => Ok(()),
        Err(e) => Err(format!(
            "Failed to verify {label} deployment at {addr}: {e}"
        )),
    }
}
