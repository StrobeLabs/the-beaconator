//! ECDSA verifier adapter on-chain deployment
//!
//! Deploys ECDSAVerifierAdapter contracts using pre-compiled bytecode,
//! setting the beaconator's PRIVATE_KEY signer as the designated signer.

use alloy::primitives::{Address, Bytes};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use alloy::sol_types::SolValue;
use std::time::Duration;
use tokio::time::timeout;

use crate::models::AppState;
use crate::services::wallet::WalletHandle;

/// Deploys an ECDSAVerifierAdapter contract with the beaconator's PRIVATE_KEY signer.
///
/// Uses the provided wallet handle's provider to send the deployment transaction.
/// The deployed verifier will only accept signatures from `state.signer.address()`.
pub async fn deploy_ecdsa_verifier_adapter(
    state: &AppState,
    wallet_handle: &WalletHandle,
) -> Result<Address, String> {
    let signer_address = state.signer.address();
    tracing::info!(
        "Deploying ECDSAVerifierAdapter with signer={}",
        signer_address
    );

    // Build provider from wallet handle
    let provider = wallet_handle
        .build_provider(&state.rpc_url)
        .map_err(|e| format!("Failed to build provider for verifier deployment: {e}"))?;

    if state.ecdsa_verifier_adapter_bytecode.is_empty() {
        return Err(
            "ECDSAVerifierAdapter bytecode is empty - check abis/ECDSAVerifierAdapter.bytecode"
                .to_string(),
        );
    }

    // ABI-encode constructor arg: address _signer
    let constructor_args = signer_address.abi_encode();

    // Concatenate bytecode + constructor args
    let mut deploy_data = state.ecdsa_verifier_adapter_bytecode.to_vec();
    deploy_data.extend_from_slice(&constructor_args);

    // Build deployment transaction (to = None for contract creation)
    let tx = TransactionRequest::default().input(Bytes::from(deploy_data).into());

    // Send deployment transaction
    let pending_tx = provider
        .send_transaction(tx)
        .await
        .map_err(|e| format!("Failed to send verifier deployment transaction: {e}"))?;

    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("Verifier deployment tx sent: {:?}", tx_hash);

    // Wait for receipt
    let receipt = match timeout(Duration::from_secs(120), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => receipt,
        Ok(Err(e)) => {
            return Err(format!("Failed to get verifier deployment receipt: {e}"));
        }
        Err(_) => {
            return Err(format!(
                "Timeout waiting for verifier deployment receipt (tx: {tx_hash})"
            ));
        }
    };

    // Check transaction status
    if !receipt.status() {
        return Err(format!(
            "Verifier deployment transaction {tx_hash} reverted"
        ));
    }

    // Extract deployed contract address
    let verifier_address = receipt.contract_address.ok_or_else(|| {
        format!("Verifier deployment receipt missing contract_address (tx: {tx_hash})")
    })?;

    tracing::info!(
        "ECDSAVerifierAdapter deployed at {} (signer={})",
        verifier_address,
        signer_address
    );

    Ok(verifier_address)
}
