//! Best-effort funding refresh: after an ECDSA beacon update lands on-chain,
//! `touch()` the perp(s) backing that beacon so funding and EMAs refresh with
//! the new index.
//!
//! Flow: the update handler drops the beacon address into a bounded channel via
//! [`TouchDispatcher::dispatch`] (non-blocking; never affects the update
//! response). A single background [`TouchWorker`] resolves each beacon to its
//! perps (via perpcity-bot-api, cached), coalesces + de-duplicates the perps
//! across all beacons in a short flush window, and sends them as batched
//! `Multicall3.aggregate3(allowFailure)` `touch()` transactions.
//!
//! The whole feature is gated behind `TOUCH_ON_UPDATE_ENABLED` (default off) and
//! degrades to a no-op if required config is missing - a touch misconfiguration
//! must never take down beacon updates.

mod resolver;
mod worker;

pub use resolver::{
    PerpResolver, dedup_preserving_order, entry_is_fresh, markets_url,
    parse_perp_addresses_from_json,
};
pub use worker::{TouchWorker, touch_batch_gas_limit, touch_calldata, touch_calls};

use std::env;
use std::sync::Arc;
use std::time::Duration;

use alloy::primitives::Address;
use tokio::sync::mpsc;

use crate::services::wallet::WalletManager;

/// Bounded queue depth of pending beacon signals. A full channel means the
/// worker is behind; further signals are dropped (best-effort) rather than
/// growing memory without bound.
const CHANNEL_CAPACITY: usize = 8192;

const DEFAULT_FLUSH_INTERVAL_MS: u64 = 1000;
const DEFAULT_MAX_BATCH: usize = 50;
const DEFAULT_MAPPING_TTL_SECS: u64 = 300;
const DEFAULT_MAPPING_EMPTY_TTL_SECS: u64 = 60;

/// Handle the update path uses to enqueue a beacon whose perps should be
/// touched. Cheap to clone; [`TouchDispatcher::disabled`] is a no-op used when
/// the feature is off and in tests.
#[derive(Clone)]
pub struct TouchDispatcher {
    tx: Option<mpsc::Sender<Address>>,
}

impl TouchDispatcher {
    /// A no-op dispatcher (feature disabled).
    pub fn disabled() -> Self {
        Self { tx: None }
    }

    fn enabled(tx: mpsc::Sender<Address>) -> Self {
        Self { tx: Some(tx) }
    }

    /// Non-blocking: enqueue `beacon` for a follow-up touch of its perps. Never
    /// blocks the caller and never fails the update path.
    pub fn dispatch(&self, beacon: Address) {
        let Some(tx) = &self.tx else {
            return;
        };
        match tx.try_send(beacon) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!(
                    target: "touch",
                    metric = "TouchDropped",
                    %beacon,
                    "touch queue full; dropping beacon signal (worker behind)"
                );
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::error!(
                    target: "touch",
                    metric = "TouchWorkerClosed",
                    %beacon,
                    "touch worker channel closed; touch is disabled for this process"
                );
            }
        }
    }
}

/// Build the dispatcher and, when enabled and fully configured, spawn the touch
/// worker. Returns a disabled (no-op) dispatcher when the flag is off, or when
/// it is on but required config is missing/invalid - beacon updates must keep
/// working even if the touch side-loop is misconfigured, so it degrades to
/// "off" with a loud error rather than panicking.
///
/// Must be called from within the tokio runtime (it may `tokio::spawn`).
pub fn spawn_from_env(
    manager: Arc<WalletManager>,
    rpc_url: String,
    multicall3: Option<Address>,
) -> TouchDispatcher {
    if !env_bool("TOUCH_ON_UPDATE_ENABLED", false) {
        tracing::info!(target: "touch", "TOUCH_ON_UPDATE_ENABLED is off; not touching perps on update");
        return TouchDispatcher::disabled();
    }

    let Some(bot_api_url) = env_nonempty("BOT_API_URL") else {
        tracing::error!(
            target: "touch",
            "TOUCH_ON_UPDATE_ENABLED is on but BOT_API_URL is not set; touch disabled"
        );
        return TouchDispatcher::disabled();
    };
    let Some(bot_api_key) = env_nonempty("BOT_API_KEY") else {
        tracing::error!(
            target: "touch",
            "TOUCH_ON_UPDATE_ENABLED is on but BOT_API_KEY is not set; touch disabled"
        );
        return TouchDispatcher::disabled();
    };
    let Some(multicall3) = multicall3 else {
        tracing::error!(
            target: "touch",
            "TOUCH_ON_UPDATE_ENABLED is on but MULTICALL3_ADDRESS is not set; touch disabled"
        );
        return TouchDispatcher::disabled();
    };

    // Floor to 1ms: tokio::time::interval panics on a zero period, which would
    // kill the worker (while leaving the dispatcher enabled until the channel
    // backs up). A 0 here can only come from an operator setting the env to 0.
    let flush_interval = Duration::from_millis(
        env_parse("TOUCH_FLUSH_INTERVAL_MS", DEFAULT_FLUSH_INTERVAL_MS).max(1),
    );
    let max_batch = env_parse("TOUCH_MAX_BATCH", DEFAULT_MAX_BATCH).max(1);
    let mapping_ttl = Duration::from_secs(env_parse(
        "TOUCH_MAPPING_TTL_SECONDS",
        DEFAULT_MAPPING_TTL_SECS,
    ));
    let mapping_empty_ttl = Duration::from_secs(env_parse(
        "TOUCH_MAPPING_EMPTY_TTL_SECONDS",
        DEFAULT_MAPPING_EMPTY_TTL_SECS,
    ));

    let resolver = match PerpResolver::new(bot_api_url, bot_api_key, mapping_ttl, mapping_empty_ttl)
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(target: "touch", error = %e, "failed to build perp resolver; touch disabled");
            return TouchDispatcher::disabled();
        }
    };

    let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
    let worker = TouchWorker::new(
        rx,
        resolver,
        manager,
        rpc_url,
        multicall3,
        flush_interval,
        max_batch,
    );
    tokio::spawn(worker.run());

    tracing::info!(
        target: "touch",
        flush_interval_ms = flush_interval.as_millis() as u64,
        max_batch,
        mapping_ttl_secs = mapping_ttl.as_secs(),
        "touch-on-update enabled: worker started"
    );
    TouchDispatcher::enabled(tx)
}

fn env_bool(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

fn env_nonempty(key: &str) -> Option<String> {
    env::var(key).ok().and_then(|v| {
        let t = v.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    })
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<T>().ok())
        .unwrap_or(default)
}
