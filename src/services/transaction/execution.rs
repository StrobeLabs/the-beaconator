//! Transaction execution utilities
//!
//! This module provides helper functions for transaction execution:
//! - `is_nonce_error`: Detect nonce-related errors in error messages
//! - `is_insufficient_funds_error`: Detect drained-wallet errors
//! - `send_with_nonce_retry`: Send a wallet transaction, retrying nonce errors
//!
//! Note: Transaction serialization is now handled by Redis-based distributed
//! locks in the wallet module. See `WalletLock` for details.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

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

/// Backoff between retries after a nonce error, giving lagging RPC replicas
/// time to execute the block that confirmed the wallet's previous transaction.
const NONCE_RETRY_DELAYS_MS: [u64; 2] = [300, 900];

/// The boxed transaction-send future returned by a retryable send closure.
pub type SendFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;

/// Send a wallet transaction, retrying on nonce errors.
///
/// Every send builds a fresh alloy provider whose nonce filler queries the RPC
/// for the wallet's transaction count. Right after the wallet's previous
/// transaction confirms, a load-balanced RPC can serve that count from a
/// replica that has not executed the confirming block yet, so the next send
/// reuses the spent nonce and the sequencer rejects it ("nonce too low:
/// tx N, state N+1" -- observed on the production create path 2026-08-24,
/// where the verifier deploy confirmed and the beacon deploy then failed).
///
/// The wallet pool's per-wallet lock guarantees no concurrent sender, so
/// retrying is safe: each attempt re-queries the nonce and converges as soon
/// as any replica has caught up. `send` is invoked once per attempt and must
/// rebuild the transaction (including any per-attempt lock check); only
/// errors matching `is_nonce_error` are retried, everything else returns
/// immediately.
pub async fn send_with_nonce_retry<'a, T, F>(op_name: &str, mut send: F) -> Result<T, String>
where
    F: FnMut() -> SendFuture<'a, T> + Send,
{
    let attempts = NONCE_RETRY_DELAYS_MS.len() + 1;
    let mut last_err = String::new();
    for (attempt, delay_ms) in NONCE_RETRY_DELAYS_MS
        .iter()
        .copied()
        .map(Some)
        .chain(std::iter::once(None))
        .enumerate()
    {
        match send().await {
            Ok(value) => return Ok(value),
            Err(e) if is_nonce_error(&e) => {
                last_err = e;
                if let Some(delay_ms) = delay_ms {
                    tracing::warn!(
                        "{op_name}: nonce error on attempt {}/{attempts}, retrying in {delay_ms}ms: {last_err}",
                        attempt + 1,
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                } else {
                    tracing::error!(
                        "{op_name}: nonce error persisted through {attempts} attempts: {last_err}"
                    );
                }
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err)
}

// Tests moved to tests/unit_tests/transaction_execution_tests.rs
