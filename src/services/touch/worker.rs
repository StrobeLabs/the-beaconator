//! Coalescing touch worker.
//!
//! A single long-lived task receives beacon addresses (one per confirmed ECDSA
//! update), resolves each to its perp(s), and accumulates a de-duplicated set of
//! perps. On a flush interval (or when the set reaches `max_batch`) it sends the
//! perps as batched `Multicall3.aggregate3(allowFailure = true)` `touch()`
//! transactions from a pool wallet.
//!
//! `touch()` is time-integrated (it accrues funding over `dt` since `lastTouch`
//! and reads the current `index()` live), so touching a perp once per flush
//! window captures the same funding total as touching once per update, at a
//! fraction of the tx count. Everything here is best-effort: a failure is logged
//! and the perps are re-driven by subsequent updates.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use alloy::primitives::{Address, Bytes};
use alloy::sol_types::SolCall;
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval, timeout};

use crate::routes::{IMulticall3, IPerp};
use crate::services::wallet::WalletManager;

use super::resolver::PerpResolver;

/// Bounded wait for a touch batch receipt, purely to record per-perp
/// success/failure metrics. The transaction lands regardless; this only bounds
/// how long the (single) worker blocks per flush.
const RECEIPT_TIMEOUT: Duration = Duration::from_secs(30);

pub struct TouchWorker {
    rx: mpsc::Receiver<Address>,
    resolver: PerpResolver,
    manager: Arc<WalletManager>,
    rpc_url: String,
    multicall3: Address,
    flush_interval: Duration,
    max_batch: usize,
}

impl TouchWorker {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rx: mpsc::Receiver<Address>,
        resolver: PerpResolver,
        manager: Arc<WalletManager>,
        rpc_url: String,
        multicall3: Address,
        flush_interval: Duration,
        max_batch: usize,
    ) -> Self {
        Self {
            rx,
            resolver,
            manager,
            rpc_url,
            multicall3,
            flush_interval,
            max_batch: max_batch.max(1),
        }
    }

    /// Run until the channel closes (all senders dropped), which only happens at
    /// shutdown. No per-iteration error escapes the loop.
    pub async fn run(mut self) {
        let mut pending: HashSet<Address> = HashSet::new();
        let mut tick = interval(self.flush_interval);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        // interval's first tick is immediate; consume it so the first real flush
        // waits a full window.
        tick.tick().await;

        loop {
            tokio::select! {
                maybe = self.rx.recv() => match maybe {
                    Some(beacon) => {
                        for perp in self.resolver.resolve_perps(beacon).await {
                            pending.insert(perp);
                        }
                        if pending.len() >= self.max_batch {
                            self.flush(&mut pending).await;
                        }
                    }
                    None => {
                        if !pending.is_empty() {
                            self.flush(&mut pending).await;
                        }
                        tracing::info!(target: "touch", "touch worker channel closed; exiting");
                        return;
                    }
                },
                _ = tick.tick() => {
                    if !pending.is_empty() {
                        self.flush(&mut pending).await;
                    }
                }
            }
        }
    }

    /// Send all currently-pending perps as batched touch() transactions from a
    /// single pool wallet. Drains `pending` up front; on any acquisition/build
    /// failure the batch is dropped (best-effort) and subsequent updates
    /// re-enqueue the perps.
    async fn flush(&self, pending: &mut HashSet<Address>) {
        let perps: Vec<Address> = pending.drain().collect();
        if perps.is_empty() {
            return;
        }

        let handle = match self.manager.acquire_any_wallet().await {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!(
                    target: "touch",
                    metric = "TouchWalletUnavailable",
                    perps = perps.len(),
                    error = %e,
                    "no pool wallet available for touch batch; dropping (updates will re-drive)"
                );
                return;
            }
        };

        let provider = match handle.build_provider(&self.rpc_url) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    target: "touch",
                    error = %e,
                    "failed to build provider for touch batch; dropping"
                );
                return;
            }
        };

        let multicall = IMulticall3::new(self.multicall3, &provider);

        for chunk in perps.chunks(self.max_batch) {
            // A lost distributed lock means another instance may be using this
            // wallet; sending now would risk a nonce collision.
            if let Err(e) = handle.ensure_lock_held() {
                tracing::warn!(
                    target: "touch",
                    error = %e,
                    "lost wallet lock before touch batch; aborting flush"
                );
                return;
            }

            let calls = touch_calls(chunk);
            match multicall.aggregate3(calls).send().await {
                Ok(pending_tx) => {
                    let tx_hash = *pending_tx.tx_hash();
                    tracing::info!(
                        target: "touch",
                        metric = "TouchBatchSent",
                        perps = chunk.len(),
                        tx = ?tx_hash,
                        "sent touch batch"
                    );
                    match timeout(RECEIPT_TIMEOUT, pending_tx.get_receipt()).await {
                        Ok(Ok(receipt)) if receipt.status() => {
                            // With allowFailure=true a sub-call can revert
                            // silently; a successful touch() emits events, so a
                            // log from the perp's address is our success signal
                            // (same technique as batch.rs IndexUpdated checks).
                            for perp in chunk {
                                let touched =
                                    receipt.inner.logs().iter().any(|l| l.address() == *perp);
                                if touched {
                                    tracing::debug!(
                                        target: "touch",
                                        metric = "TouchPerpSuccess",
                                        %perp,
                                        "perp touched"
                                    );
                                } else {
                                    tracing::warn!(
                                        target: "touch",
                                        metric = "TouchPerpFailure",
                                        %perp,
                                        tx = ?tx_hash,
                                        "touch produced no logs (sub-call may have reverted)"
                                    );
                                }
                            }
                        }
                        Ok(Ok(receipt)) => tracing::warn!(
                            target: "touch",
                            metric = "TouchBatchFailure",
                            tx = ?receipt.transaction_hash,
                            "touch batch transaction reverted"
                        ),
                        Ok(Err(e)) => tracing::warn!(
                            target: "touch",
                            metric = "TouchBatchFailure",
                            tx = ?tx_hash,
                            error = %e,
                            "failed to get touch batch receipt (may still confirm)"
                        ),
                        Err(_) => tracing::warn!(
                            target: "touch",
                            metric = "TouchBatchFailure",
                            tx = ?tx_hash,
                            "timed out waiting for touch batch receipt (may still confirm)"
                        ),
                    }
                }
                Err(e) => tracing::warn!(
                    target: "touch",
                    metric = "TouchBatchFailure",
                    perps = chunk.len(),
                    error = %e,
                    "failed to send touch batch (RpcError usually = wallet out of native gas)"
                ),
            }
        }
    }
}

// ---- pure helpers (unit-tested from tests/unit_tests/touch_tests.rs) ----

/// Calldata for `Perp.touch()` (selector 0xa55526db, no arguments).
pub fn touch_calldata() -> Bytes {
    Bytes::from(SolCall::abi_encode(&IPerp::touchCall {}))
}

/// One `allowFailure` multicall entry per perp, each calling `touch()`.
pub fn touch_calls(perps: &[Address]) -> Vec<IMulticall3::Call3> {
    let data = touch_calldata();
    perps
        .iter()
        .map(|perp| IMulticall3::Call3 {
            target: *perp,
            allowFailure: true,
            callData: data.clone(),
        })
        .collect()
}
