//! Transaction execution utilities
//!
//! This module provides helper functions for transaction execution:
//! - `is_nonce_error`: Detect nonce-related errors in error messages
//! - `resynced_nonce`: Re-read a wallet's nonce from the chain for a
//!   one-shot retry after a nonce-too-low send failure
//!
//! Note: Transaction serialization is now handled by Redis-based distributed
//! locks in the wallet module. See `WalletLock` for details.

use alloy::primitives::Address;
use alloy::providers::Provider;

/// Detect nonce-related errors from error messages
///
/// This helper function checks if an error message indicates a nonce-related issue
/// that might be resolved by syncing with an alternate RPC or retrying.
///
/// # Arguments
/// * `error_msg` - The error message to check
///
/// # Returns
/// `true` if the error is nonce-related, `false` otherwise
pub fn is_nonce_error(error_msg: &str) -> bool {
    let error_lower = error_msg.to_lowercase();
    error_lower.contains("nonce too low")
        || error_lower.contains("nonce too high")
        || error_lower.contains("invalid nonce")
        || error_lower.contains("nonce is invalid")
        || error_lower.contains("nonce is too low")
        || error_lower.contains("replacement transaction underpriced")
        || error_lower.contains("replacement tx underpriced")
}

/// Detect insufficient-funds errors from error messages
///
/// This helper function checks if an error message indicates the sending wallet
/// does not have enough native gas token to cover the transaction. A drained
/// pool wallet triggers this on send or preflight simulation; the caller can
/// use it to retry with a different wallet instead of failing the request.
///
/// # Arguments
/// * `error_msg` - The error message to check
///
/// # Returns
/// `true` if the error indicates insufficient funds, `false` otherwise
pub fn is_insufficient_funds_error(error_msg: &str) -> bool {
    let error_lower = error_msg.to_lowercase();
    error_lower.contains("insufficient funds")
        || error_lower.contains("insufficient balance for transfer")
        || error_lower.contains("gas required exceeds allowance")
}

/// Re-read a wallet's nonce from the chain's PENDING transaction count for a
/// one-shot retry after a nonce-related send failure.
///
/// Every request path builds a fresh provider, whose nonce filler seeds
/// itself with `get_transaction_count`. Behind a load-balanced RPC that read
/// can lag the head right after a prior transaction from the same wallet
/// lands, so the fill reuses the just-spent nonce and the send dies with
/// "nonce too low" (prod repro 2026-08-24: /create_beacon_with_ecdsa's
/// verifier create landed, and the beacon deployment built one round-trip
/// later carried the same nonce - tx 9055 vs state 9056). Callers classify
/// the failure with `is_nonce_error`, fetch this, and resend once with the
/// nonce set explicitly. The pending tag also counts in-flight transactions,
/// so a retry never trails a tx this process has already broadcast.
pub async fn resynced_nonce<P: Provider>(
    provider: &P,
    wallet: Address,
    what: &str,
) -> Result<u64, String> {
    let nonce = provider
        .get_transaction_count(wallet)
        .pending()
        .await
        .map_err(|e| format!("{what}: nonce resync read failed: {e}"))?;
    tracing::warn!("{what}: nonce error on send; resynced to pending count {nonce}, retrying once");
    Ok(nonce)
}

// Tests moved to tests/unit_tests/transaction_execution_tests.rs
