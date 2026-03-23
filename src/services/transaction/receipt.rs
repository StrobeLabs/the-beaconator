//! Optimized receipt polling for Base network (~2s block time).
//!
//! Replaces Alloy's default `get_receipt()` polling which may poll too aggressively,
//! wasting RPC compute units. This implementation waits 2s before the first poll
//! (matching Base block time) and then polls every 3s.

use alloy::primitives::TxHash;
use alloy::providers::Provider;
use alloy::rpc::types::TransactionReceipt;
use std::time::{Duration, Instant};

/// Poll for a transaction receipt with intervals tuned to Base's ~2s block time.
///
/// Waits 2s before the first poll, then retries every 3s until the receipt is found
/// or the timeout is reached. Returns the receipt on success.
pub async fn poll_for_receipt(
    provider: &impl Provider,
    tx_hash: TxHash,
    timeout_secs: u64,
) -> Result<TransactionReceipt, String> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);

    // Initial wait: ~1 Base block time before first poll
    tokio::time::sleep(Duration::from_secs(2)).await;

    loop {
        match provider.get_transaction_receipt(tx_hash).await {
            Ok(Some(receipt)) => {
                tracing::info!("Transaction {tx_hash} confirmed via receipt poll");
                return Ok(receipt);
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    return Err(format!(
                        "Timeout waiting for transaction {tx_hash} receipt after {timeout_secs}s"
                    ));
                }
                // Poll every 3s (slightly more than Base block time to avoid wasted polls)
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
            Err(e) => {
                return Err(format!(
                    "RPC error polling for transaction {tx_hash} receipt: {e}"
                ));
            }
        }
    }
}

/// Poll for a receipt and check that the transaction succeeded (did not revert).
///
/// Convenience wrapper over `poll_for_receipt` that also validates the receipt status.
pub async fn poll_for_successful_receipt(
    provider: &impl Provider,
    tx_hash: TxHash,
    description: &str,
    timeout_secs: u64,
) -> Result<TransactionReceipt, String> {
    let receipt = poll_for_receipt(provider, tx_hash, timeout_secs).await?;

    if !receipt.status() {
        return Err(format!("{description} transaction {tx_hash} reverted"));
    }

    Ok(receipt)
}
