use alloy::providers::Provider;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;
use tracing;

use crate::models::AppState;

/// Global transaction mutex to serialize ALL blockchain transactions
/// This prevents nonce conflicts by ensuring only one transaction is submitted at a time
static TRANSACTION_MUTEX: OnceLock<Arc<Mutex<()>>> = OnceLock::new();

/// Get the global transaction mutex
pub fn get_transaction_mutex() -> &'static Arc<Mutex<()>> {
    TRANSACTION_MUTEX.get_or_init(|| Arc::new(Mutex::new(())))
}

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

/// Serialized transaction execution wrapper
///
/// All blockchain transactions should use this to prevent nonce conflicts.
/// Alloy's wallet provider handles nonce management automatically, but we need
/// to ensure only one transaction is submitted at a time to avoid race conditions.
///
/// # Arguments
/// * `operation` - The async operation to execute (typically a transaction send)
///
/// # Returns
/// The result of the operation
///
/// # Example
/// ```ignore
/// let receipt = execute_transaction_serialized(async {
///     contract.someFunction().send().await
/// }).await?;
/// ```
pub async fn execute_transaction_serialized<F, T>(operation: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let mutex = get_transaction_mutex();
    let _lock = mutex.lock().await;
    tracing::debug!("Acquired transaction lock - executing blockchain operation serially");
    let result = operation.await;
    tracing::debug!("Released transaction lock - blockchain operation completed");
    result
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
