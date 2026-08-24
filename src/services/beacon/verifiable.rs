//! IdentityBeacon deployment via bytecode
//!
//! Deploys IdentityBeacon contracts using pre-compiled bytecode with
//! constructor args (IVerifier verifier, uint256 initialIndex).

use alloy::network::TransactionBuilder;
use alloy::primitives::{Address, Bytes, U256};
use alloy::providers::Provider;
use alloy::rpc::types::TransactionRequest;
use alloy::sol_types::SolValue;
use std::time::Duration;
use tokio::time::timeout;

use crate::models::AppState;
use crate::services::transaction::{is_nonce_error, resynced_nonce};
use crate::services::wallet::WalletHandle;

/// Deploys an IdentityBeacon contract with the given verifier and initial index.
///
/// Uses bytecode from `state.contracts.identity_beacon_bytecode` with ABI-encoded constructor args.
pub async fn deploy_identity_beacon(
    state: &AppState,
    wallet_handle: &WalletHandle,
    verifier_address: Address,
    initial_index: u128,
) -> Result<Address, String> {
    tracing::info!(
        "Deploying IdentityBeacon with verifier={}, initialIndex={}",
        verifier_address,
        initial_index
    );

    // Build provider from wallet handle
    let provider = wallet_handle
        .build_provider(&state.provider.rpc_url)
        .map_err(|e| format!("Failed to build provider for beacon deployment: {e}"))?;

    if state.contracts.identity_beacon_bytecode.is_empty() {
        return Err(
            "IdentityBeacon bytecode is empty - check abis/IdentityBeacon.bytecode".to_string(),
        );
    }

    // ABI-encode constructor args: (address _verifier, uint256 _initialIndex)
    let constructor_args = (verifier_address, U256::from(initial_index)).abi_encode();

    // Concatenate bytecode + constructor args
    let mut deploy_data = state.contracts.identity_beacon_bytecode.to_vec();
    deploy_data.extend_from_slice(&constructor_args);

    // Build deployment transaction using with_deploy_code for proper contract creation
    let tx = TransactionRequest::default().with_deploy_code(Bytes::from(deploy_data));

    // Send deployment transaction. On a nonce-too-low failure (fresh provider
    // seeded from a lagging RPC read right after the verifier tx landed - the
    // 2026-08-24 prod failure mode), resync from pending count and retry once.
    wallet_handle.ensure_lock_held()?;
    let pending_tx = match provider.send_transaction(tx.clone()).await {
        Ok(pending) => pending,
        Err(e) => {
            let first = format!("{e}");
            if !is_nonce_error(&first) {
                return Err(format!(
                    "Failed to send beacon deployment transaction: {first}"
                ));
            }
            let nonce =
                resynced_nonce(&provider, wallet_handle.address(), "IdentityBeacon deploy").await?;
            provider
                .send_transaction(tx.with_nonce(nonce))
                .await
                .map_err(|e| {
                    format!(
                        "Failed to send beacon deployment transaction after nonce resync \
                         (first attempt: {first}): {e}"
                    )
                })?
        }
    };

    let tx_hash = *pending_tx.tx_hash();
    tracing::info!("Beacon deployment tx sent: {:?}", tx_hash);

    // Wait for receipt
    let receipt = match timeout(Duration::from_secs(120), pending_tx.get_receipt()).await {
        Ok(Ok(receipt)) => receipt,
        Ok(Err(e)) => {
            return Err(format!("Failed to get beacon deployment receipt: {e}"));
        }
        Err(_) => {
            return Err(format!(
                "Timeout waiting for beacon deployment receipt (tx: {tx_hash})"
            ));
        }
    };

    // Check transaction status
    if !receipt.status() {
        return Err(format!("Beacon deployment transaction {tx_hash} reverted"));
    }

    // Extract deployed contract address
    let beacon_address = receipt.contract_address.ok_or_else(|| {
        format!("Beacon deployment receipt missing contract_address (tx: {tx_hash})")
    })?;

    tracing::info!(
        "IdentityBeacon deployed at {} (verifier={}, initialIndex={})",
        beacon_address,
        verifier_address,
        initial_index
    );

    Ok(beacon_address)
}
