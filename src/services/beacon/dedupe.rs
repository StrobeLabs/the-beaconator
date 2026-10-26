//! Publish gating for beacon updates: a rate limit and a value-dedupe.
//!
//! Two independent guards decide whether an update actually costs a
//! transaction:
//!
//! 1. **Rate limit** ([`publish_interval`]) — a floor on the time between
//!    two publishes for a beacon, applied whether or not the value changed.
//!    Off by default. It exists because an updater pushing faster than its
//!    market needs is pure gas burn: on testnet in 2026-09 the fastest beacon
//!    was publishing every 72s and alone accounted for an eighth of the pool's
//!    entire gas bill.
//! 2. **Value dedupe** ([`heartbeat_window`]) — skips the transaction when the
//!    requested measurement equals the value already on-chain, unless the
//!    per-beacon heartbeat window has elapsed. The heartbeat keeps a periodic
//!    unchanged print flowing because downstream consumers depend on print
//!    freshness (TWAP/funding liveness, the chart staleness `degraded` flag).
//!
//! Both read the same last-publish timestamp, which lives in Redis under the
//! wallet-pool namespace. Every failure path (no pool, Redis down, missing
//! key, bad value) reports `None` and the caller publishes — these guards can
//! only ever suppress a redundant update, never lose a real one.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use alloy::primitives::Address;
use redis::AsyncCommands;

use crate::models::AppState;
use crate::services::touch::Tier;

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

/// Per-beacon floor on time between publishes, applied even when the value
/// changed. `0` (the default) disables the rate limit entirely.
const MIN_INTERVAL_ENV: &str = "BEACON_MIN_PUBLISH_INTERVAL_SECONDS";
/// Per-beacon overrides for [`MIN_INTERVAL_ENV`], as a comma-separated list of
/// `address=seconds` pairs. Used to hold rarely-consumed beacons to a slower
/// cadence than the fleet default without touching their updater.
const MIN_INTERVAL_OVERRIDES_ENV: &str = "BEACON_MIN_PUBLISH_INTERVAL_OVERRIDES";

/// Per-tier intervals, e.g. `active=280,idle=900,dormant=3600`. A tier absent
/// from the map falls through to the global floor, so a partial setting is
/// still coherent.
const TIER_INTERVALS_ENV: &str = "BEACON_TIER_INTERVALS";

/// The publish rate limit for `beacon`, in precedence order:
///
/// 1. an explicit per-beacon override ([`MIN_INTERVAL_OVERRIDES_ENV`]) — the
///    manual escape hatch, always wins;
/// 2. the interval for the beacon's usage [`Tier`], when one is cached and
///    configured — this is the part that follows real usage without anyone
///    maintaining a list;
/// 3. the global floor ([`MIN_INTERVAL_ENV`]);
/// 4. `None` — no rate limit.
///
/// `tier` is whatever the caller could read without I/O; `None` (touch
/// disabled, or nothing cached yet) simply falls through to the global floor.
pub fn publish_interval(beacon: &Address, tier: Option<Tier>) -> Option<Duration> {
    if let Some(explicit) = overrides().get(beacon) {
        return Some(*explicit);
    }
    if let Some(t) = tier
        && let Some(d) = tier_intervals().get(t.as_str())
    {
        return Some(*d);
    }
    *global_interval()
}

fn tier_intervals() -> &'static HashMap<String, Duration> {
    static MAP: LazyLock<HashMap<String, Duration>> =
        LazyLock::new(|| parse_tier_intervals(std::env::var(TIER_INTERVALS_ENV).ok().as_deref()));
    &MAP
}

/// Parse `active=280,idle=900,dormant=3600`. Unknown tier names and
/// non-positive values are skipped with a warning; the rest still apply.
fn parse_tier_intervals(raw: Option<&str>) -> HashMap<String, Duration> {
    const KNOWN: [&str; 3] = ["active", "idle", "dormant"];
    let mut out = HashMap::new();
    let Some(raw) = raw else {
        return out;
    };
    for entry in raw.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let Some((name, secs)) = entry.split_once('=') else {
            tracing::warn!("Ignoring {TIER_INTERVALS_ENV} entry {entry:?}: expected tier=seconds");
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        match (KNOWN.contains(&name.as_str()), secs.trim().parse::<u64>()) {
            (true, Ok(s)) if s > 0 => {
                out.insert(name, Duration::from_secs(s));
            }
            (false, _) => {
                tracing::warn!("Ignoring {TIER_INTERVALS_ENV} entry {entry:?}: unknown tier");
            }
            _ => {
                tracing::warn!(
                    "Ignoring {TIER_INTERVALS_ENV} entry {entry:?}: seconds must be > 0"
                );
            }
        }
    }
    out
}

fn global_interval() -> &'static Option<Duration> {
    static GLOBAL: LazyLock<Option<Duration>> =
        LazyLock::new(|| parse_interval(std::env::var(MIN_INTERVAL_ENV).ok().as_deref()));
    &GLOBAL
}

fn overrides() -> &'static HashMap<Address, Duration> {
    static OVERRIDES: LazyLock<HashMap<Address, Duration>> = LazyLock::new(|| {
        parse_overrides(std::env::var(MIN_INTERVAL_OVERRIDES_ENV).ok().as_deref())
    });
    &OVERRIDES
}

/// The publish rate limit for `beacon` ignoring usage tiers: the per-beacon
/// override when one is configured, else the global floor, else `None`.
///
/// Parsed once per process — these are deployment config, not runtime state.
///
/// **Set the interval below the updater's cadence, not equal to it.** The
/// last-publish record has whole-second resolution, so an updater pushing every
/// `N`s yields an `elapsed` of `N` or `N-1` depending on where the two clocks
/// round. A floor of exactly `N` therefore skips on the `N-1` ticks and
/// stretches that beacon to two cycles at random — the same boundary race
/// [`DEFAULT_HEARTBEAT`] avoids by sitting at 840s against a 900s cadence.
/// Leaving a few percent of headroom makes the limit deterministic; the extra
/// publishes it allows are far cheaper than an index that silently halves its
/// refresh rate.
pub fn min_publish_interval(beacon: &Address) -> Option<Duration> {
    overrides().get(beacon).copied().or(*global_interval())
}

/// The longest interval this process could ever apply to any beacon. Used to
/// size the last-publish TTL: a record that expires before the interval it
/// gates would let the beacon publish early, so the TTL must clear the
/// slowest tier, not just this beacon's current one (its tier can change).
fn max_configured_interval() -> Option<Duration> {
    heartbeat_window()
        .into_iter()
        .chain(*global_interval())
        .chain(overrides().values().copied())
        .chain(tier_intervals().values().copied())
        .max()
}

/// `None`/absent/`0`/garbage all mean "no rate limit" — unlike the heartbeat,
/// this guard has no safe default, so anything we cannot read as a positive
/// number leaves behavior unchanged.
fn parse_interval(raw: Option<&str>) -> Option<Duration> {
    let v = raw?.trim();
    match v.parse::<u64>() {
        Ok(0) => None,
        Ok(secs) => Some(Duration::from_secs(secs)),
        Err(_) => {
            tracing::warn!("Invalid {MIN_INTERVAL_ENV}={v:?}; publish rate limit disabled");
            None
        }
    }
}

/// Parse `addr=secs,addr=secs`. A malformed entry is skipped with a warning
/// rather than discarding the whole map, so one bad address cannot silently
/// remove the rate limit from every other beacon.
fn parse_overrides(raw: Option<&str>) -> HashMap<Address, Duration> {
    let mut out = HashMap::new();
    let Some(raw) = raw else {
        return out;
    };
    for entry in raw.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let Some((addr, secs)) = entry.split_once('=') else {
            tracing::warn!(
                "Ignoring {MIN_INTERVAL_OVERRIDES_ENV} entry {entry:?}: expected addr=seconds"
            );
            continue;
        };
        match (addr.trim().parse::<Address>(), secs.trim().parse::<u64>()) {
            (Ok(a), Ok(s)) if s > 0 => {
                out.insert(a, Duration::from_secs(s));
            }
            (Ok(_), Ok(_)) => {
                tracing::warn!(
                    "Ignoring {MIN_INTERVAL_OVERRIDES_ENV} entry {entry:?}: seconds must be > 0"
                );
            }
            _ => {
                tracing::warn!("Ignoring malformed {MIN_INTERVAL_OVERRIDES_ENV} entry {entry:?}");
            }
        }
    }
    out
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
///
/// Both guards read this record, so it is written whenever *either* is active
/// and the TTL is sized from the longest interval configured anywhere (see
/// [`max_configured_interval`]) — otherwise disabling the heartbeat, or a
/// beacon moving to a slower tier, would silently blind the rate limit.
pub async fn record_publish(state: &AppState, beacon: &Address) {
    let Some(window) = max_configured_interval() else {
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

    // ── publish rate limit ──────────────────────────────────────────────

    #[test]
    fn interval_absent_or_zero_disables_rate_limit() {
        assert_eq!(parse_interval(None), None);
        assert_eq!(parse_interval(Some("0")), None);
    }

    #[test]
    fn interval_parses_and_trims() {
        assert_eq!(
            parse_interval(Some(" 300 ")),
            Some(Duration::from_secs(300))
        );
    }

    /// Unlike the heartbeat, garbage must NOT fall back to a non-zero default:
    /// a typo in deployment config must not start suppressing publishes.
    #[test]
    fn interval_garbage_disables_rather_than_defaults() {
        assert_eq!(parse_interval(Some("5m")), None);
    }

    #[test]
    fn overrides_parse_pairs() {
        let m = parse_overrides(Some(
            "0x0000000000000000000000000000000000000001=3600,\
             0x0000000000000000000000000000000000000002=900",
        ));
        let one: Address = "0x0000000000000000000000000000000000000001"
            .parse()
            .unwrap();
        let two: Address = "0x0000000000000000000000000000000000000002"
            .parse()
            .unwrap();
        assert_eq!(m.get(&one), Some(&Duration::from_secs(3600)));
        assert_eq!(m.get(&two), Some(&Duration::from_secs(900)));
    }

    #[test]
    fn overrides_skip_bad_entries_but_keep_good_ones() {
        let m = parse_overrides(Some(
            "not-an-address=60,\
             0x0000000000000000000000000000000000000003=120,\
             0x0000000000000000000000000000000000000004=0,\
             missing-equals",
        ));
        let three: Address = "0x0000000000000000000000000000000000000003"
            .parse()
            .unwrap();
        assert_eq!(m.len(), 1, "only the well-formed positive entry survives");
        assert_eq!(m.get(&three), Some(&Duration::from_secs(120)));
    }

    // ── tier intervals ──────────────────────────────────────────────────

    #[test]
    fn tier_intervals_parse() {
        let m = parse_tier_intervals(Some("active=280,idle=900,dormant=3600"));
        assert_eq!(m.get("active"), Some(&Duration::from_secs(280)));
        assert_eq!(m.get("idle"), Some(&Duration::from_secs(900)));
        assert_eq!(m.get("dormant"), Some(&Duration::from_secs(3600)));
    }

    #[test]
    fn tier_intervals_are_case_insensitive_and_trimmed() {
        let m = parse_tier_intervals(Some(" Active = 280 "));
        assert_eq!(m.get("active"), Some(&Duration::from_secs(280)));
    }

    /// A typo in one tier must not silently unset the others — a partial map
    /// still applies, and the unset tier falls through to the global floor.
    #[test]
    fn tier_intervals_skip_bad_entries_but_keep_good_ones() {
        let m = parse_tier_intervals(Some("active=280,bogus=60,idle=0,dormant"));
        assert_eq!(m.len(), 1);
        assert_eq!(m.get("active"), Some(&Duration::from_secs(280)));
    }

    #[test]
    fn tier_intervals_empty_when_unset() {
        assert!(parse_tier_intervals(None).is_empty());
        assert!(parse_tier_intervals(Some("   ")).is_empty());
    }

    #[test]
    fn overrides_empty_when_unset() {
        assert!(parse_overrides(None).is_empty());
        assert!(parse_overrides(Some("  ")).is_empty());
    }
}
