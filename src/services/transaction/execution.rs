//! Transaction execution utilities
//!
//! This module provides helper functions for transaction execution:
//! - `is_nonce_error`: Detect nonce-related errors in error messages
//!
//! Note: Transaction serialization is now handled by Redis-based distributed
//! locks in the wallet module. See `WalletLock` for details.

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

// Tests moved to tests/unit_tests/transaction_execution_tests.rs
