//! Transaction execution utilities
//!
//! This module provides helper functions for transaction execution:
//! - `get_fresh_nonce_from_alternate`: Fetch nonce from alternate RPC for sync
//! - `is_nonce_error`: Detect nonce-related errors in error messages
//!
//! Note: Transaction serialization is now handled by Redis-based distributed
//! locks in the wallet module. See `WalletLock` for details.

use alloy::providers::Provider;

use crate::models::AppState;

/// Get fresh nonce from alternate RPC provider
///
/// This is useful for syncing nonce state when there are nonce conflicts
/// between primary and alternate RPC providers.
///
/// # Arguments
/// * `state` - Application state containing the alternate provider
///
/// # Returns
/// * `Ok(u64)` - Fresh nonce from alternate RPC
/// * `Err(String)` - Error message if alternate provider not available or fetch failed
pub async fn get_fresh_nonce_from_alternate(state: &AppState) -> Result<u64, String> {
    if let Some(alternate_provider) = &state.alternate_provider {
        tracing::info!("Getting fresh nonce from alternate RPC...");
        match alternate_provider
            .get_transaction_count(state.wallet_address)
            .await
        {
            Ok(nonce) => {
                tracing::info!("Fresh nonce from alternate RPC: {}", nonce);
                Ok(nonce)
            }
            Err(e) => {
                let error_msg = format!("Failed to get nonce from alternate RPC: {e}");
                tracing::error!("{}", error_msg);
                Err(error_msg)
            }
        }
    } else {
        Err("No alternate provider available".to_string())
    }
}

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

// Tests moved to tests/unit_tests/transaction_execution_tests.rs
