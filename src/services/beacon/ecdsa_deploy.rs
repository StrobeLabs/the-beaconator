//! ECDSA verifier creation via ECDSAVerifierFactory
//!
//! Creates ECDSAVerifier instances using the on-chain factory contract,
//! setting the beaconator's PRIVATE_KEY signer as the designated signer.

use alloy::primitives::Address;
use std::time::Duration;
use tokio::time::timeout;

use crate::models::AppState;
use crate::routes::IEcdsaVerifierFactory;
use crate::services::wallet::WalletHandle;

/// Creates an ECDSAVerifier via the ECDSAVerifierFactory contract.
///
/// Uses the provided wallet handle's provider to send the factory call.
/// The created verifier will only accept signatures from `state.signer.address()`.
///
/// Strategy: simulate with .call() first to get the deterministic return address,
/// then execute with .send() to actually create the contract on-chain.
pub async fn create_ecdsa_verifier(
    state: &AppState,
    wallet_handle: &WalletHandle,
) -> Result<Address, String> {
    let signer_address = state.signer.address();
    tracing::info!(
        "Creating ECDSAVerifier via factory with signer={}",
        signer_address
    );

    // Build provider from wallet handle
    let provider = wallet_handle
        .build_provider(&state.rpc_url)
        .map_err(|e| format!("Failed to build provider for verifier creation: {e}"))?;

    let factory = IEcdsaVerifierFactory::new(state.ecdsa_verifier_factory_address, &provider);

    // Simulate the call first to get the return address (deterministic via CREATE opcode)
    let simulated = factory
        .createVerifier(signer_address)
        .call()
        .await
        .map_err(|e| format!("Failed to simulate createVerifier: {e}"))?;
    let verifier_address = Address::from(simulated.0);

    tracing::info!(
        "Simulated verifier creation - expected address: {}",
        verifier_address
    );

    // Execute the actual transaction
    let pending_tx = factory
        .createVerifier(signer_address)
        .send()
        .await
        .map_err(|e| format!("Failed to send createVerifier transaction: {e}"))?;

    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("Verifier creation tx sent: {:?}", tx_hash);

    // Wait for receipt
    let receipt = match timeout(Duration::from_secs(120), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => receipt,
        Ok(Err(e)) => {
            return Err(format!("Failed to get verifier creation receipt: {e}"));
        }
        Err(_) => {
            return Err(format!(
                "Timeout waiting for verifier creation receipt (tx: {tx_hash})"
            ));
        }
    };

    // Check transaction status
    if !receipt.status() {
        return Err(format!("Verifier creation transaction {tx_hash} reverted"));
    }

    tracing::info!(
        "ECDSAVerifier created at {} (signer={})",
        verifier_address,
        signer_address
    );

    Ok(verifier_address)
}
