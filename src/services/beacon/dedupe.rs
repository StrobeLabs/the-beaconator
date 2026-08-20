//! Value-dedupe for beacon updates.
//!
//! Skips the on-chain transaction when the requested measurement equals the
//! value already on-chain, unless the per-beacon heartbeat window has elapsed.
//! The heartbeat keeps a periodic unchanged print flowing because downstream
//! consumers depend on print freshness (TWAP/funding liveness, the chart
//! staleness `degraded` flag).
//!
//! The last-publish timestamp lives in Redis under the wallet-pool namespace.
//! Every failure path (no pool, Redis down, missing key, bad value) reports
//! `None` and the caller publishes — dedupe can only ever suppress a
//! duplicate, never a real update.

use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use alloy::primitives::Address;
use redis::AsyncCommands;

use crate::models::AppState;

/// Just under the common 15-minute updater cadence, so an unchanged value
/// arriving every 900s always republishes (elapsed ≈ 900 > 840) instead of
/// racing the window boundary and stretching the heartbeat to two cycles.
const DEFAULT_HEARTBEAT: Duration = Duration::from_secs(840);
const HEARTBEAT_ENV: &str = "BEACON_UPDATE_HEARTBEAT_SECONDS";

/// Dedupe window: an unchanged value is republished only after this long.
/// `BEACON_UPDATE_HEARTBEAT_SECONDS` overrides the 900s default; `0` disables
/// dedupe entirely (every request publishes, the pre-dedupe behavior).
pub fn heartbeat_window() -> Option<Duration> {
    static WINDOW: LazyLock<Option<Duration>> =
        LazyLock::new(|| parse_window(std::env::var(HEARTBEAT_ENV).ok().as_deref()));
    *WINDOW
}

fn parse_window(raw: Option<&str>) -> Option<Duration> {
    match raw {
        None => Some(DEFAULT_HEARTBEAT),
        Some(v) => match v.trim().parse::<u64>() {
            Ok(0) => None,
            Ok(secs) => Some(Duration::from_secs(secs)),
            Err(_) => {
                tracing::warn!(
                    "Invalid {HEARTBEAT_ENV}={v:?}; using default {}s",
                    DEFAULT_HEARTBEAT.as_secs()
                );
                Some(DEFAULT_HEARTBEAT)
            }
        },
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Seconds since the last recorded publish for `beacon`, or `None` when
/// unknown (no record, no pool, Redis error) — the caller must then publish.
pub async fn seconds_since_last_publish(state: &AppState, beacon: &Address) -> Option<u64> {
    let (mut conn, keys) = state.wallets.manager.redis()?;
    let key = keys.beacon_last_publish(beacon);
    match conn.get::<_, Option<String>>(&key).await {
        Ok(Some(v)) => v
            .trim()
            .parse::<u64>()
            .ok()
            .map(|t| now_secs().saturating_sub(t)),
        Ok(None) => None,
        Err(e) => {
            tracing::warn!("Dedupe read of {key} failed: {e} — treating as no record");
            None
        }
    }
}

/// Best-effort record that an update transaction for `beacon` was sent. The
/// TTL bounds staleness of the record itself; losing it only costs one extra
/// publish.
pub async fn record_publish(state: &AppState, beacon: &Address) {
    let Some(window) = heartbeat_window() else {
        return;
    };
    let Some((mut conn, keys)) = state.wallets.manager.redis() else {
        return;
    };
    let key = keys.beacon_last_publish(beacon);
    let ttl = window.as_secs().saturating_mul(4).max(60);
    if let Err(e) = conn
        .set_ex::<_, _, ()>(&key, now_secs().to_string(), ttl)
        .await
    {
        tracing::warn!("Dedupe record of {key} failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_window_when_env_unset() {
        assert_eq!(parse_window(None), Some(DEFAULT_HEARTBEAT));
    }

    #[test]
    fn zero_disables_dedupe() {
        assert_eq!(parse_window(Some("0")), None);
    }

    #[test]
    fn explicit_window_parses() {
        assert_eq!(parse_window(Some("1800")), Some(Duration::from_secs(1800)));
    }

    #[test]
    fn garbage_falls_back_to_default() {
        assert_eq!(parse_window(Some("15m")), Some(DEFAULT_HEARTBEAT));
    }
}
