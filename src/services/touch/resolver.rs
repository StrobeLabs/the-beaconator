//! Resolve which Perp market(s) are backed by a given beacon, by querying
//! perpcity-bot-api and caching the (near-static) mapping.
//!
//! The bot-api serves the mapping from the indexed Goldsky Mirror:
//! `GET /markets?beacon=<hex>&limit=500` returns a page of markets, each with a
//! `perp_address`. One beacon can back several perps, so the result is a list.
//!
//! [`PerpResolver::resolve_perps`] never errors: on any failure it degrades to a
//! fresh-enough cached entry (stale-if-error) or an empty set, so a bot-api
//! outage never propagates into the touch worker.

use std::collections::HashMap;
use std::str::FromStr;
use std::time::{Duration, Instant};

use alloy::primitives::Address;
use serde::Deserialize;
use tokio::sync::RwLock;

/// bot-api caps `limit` at 500; request the max to minimise round trips.
const PAGE_LIMIT: usize = 500;
/// Hard cap on pages fetched per beacon, guarding against a buggy `has_more`
/// that never turns false (would otherwise loop forever).
const MAX_PAGES: usize = 40;
/// Per-request timeout so a slow bot-api cannot stall the single touch worker.
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);

/// One page of `GET /markets`. Only the fields the resolver needs are decoded;
/// everything else is ignored, and both fields default so a partial/renamed
/// payload degrades to "no perps" rather than a hard decode error.
#[derive(Debug, Deserialize)]
struct MarketsPage {
    #[serde(default)]
    items: Vec<MarketItem>,
    #[serde(default)]
    has_more: bool,
}

#[derive(Debug, Deserialize)]
struct MarketItem {
    #[serde(default)]
    perp_address: String,
}

struct CacheEntry {
    perps: Vec<Address>,
    fetched_at: Instant,
}

/// Cached beacon -> perps resolver backed by perpcity-bot-api.
pub struct PerpResolver {
    client: reqwest::Client,
    /// bot-api base, trailing slash trimmed (we request `/markets`, never
    /// `/markets/`, to avoid FastAPI's 307 redirect).
    base_url: String,
    api_key: String,
    ttl: Duration,
    empty_ttl: Duration,
    cache: RwLock<HashMap<Address, CacheEntry>>,
}

impl PerpResolver {
    pub fn new(
        base_url: String,
        api_key: String,
        ttl: Duration,
        empty_ttl: Duration,
    ) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .map_err(|e| format!("failed to build touch http client: {e}"))?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            ttl,
            empty_ttl,
            cache: RwLock::new(HashMap::new()),
        })
    }

    /// Resolve the perps backing `beacon`. Never errors: a fresh cache hit is
    /// returned directly; otherwise it fetches and, on failure, degrades to a
    /// stale cache entry (if any) or an empty set.
    pub async fn resolve_perps(&self, beacon: Address) -> Vec<Address> {
        if let Some(perps) = self.fresh_cached(beacon).await {
            return perps;
        }
        match self.fetch(beacon).await {
            Ok(perps) => {
                self.store(beacon, perps.clone()).await;
                perps
            }
            Err(e) => {
                tracing::warn!(
                    target: "touch",
                    metric = "MappingFetchError",
                    %beacon,
                    error = %e,
                    "beacon->perps fetch failed; using stale mapping if present"
                );
                self.stale(beacon).await
            }
        }
    }

    async fn fresh_cached(&self, beacon: Address) -> Option<Vec<Address>> {
        let cache = self.cache.read().await;
        let entry = cache.get(&beacon)?;
        entry_is_fresh(
            entry.fetched_at,
            entry.perps.is_empty(),
            self.ttl,
            self.empty_ttl,
            Instant::now(),
        )
        .then(|| entry.perps.clone())
    }

    async fn stale(&self, beacon: Address) -> Vec<Address> {
        self.cache
            .read()
            .await
            .get(&beacon)
            .map(|e| e.perps.clone())
            .unwrap_or_default()
    }

    async fn store(&self, beacon: Address, perps: Vec<Address>) {
        self.cache.write().await.insert(
            beacon,
            CacheEntry {
                perps,
                fetched_at: Instant::now(),
            },
        );
    }

    /// Fetch every page for `beacon`, de-duplicating perps across pages. Returns
    /// `Err` (leaving the cache untouched) if any page request/decode fails, so
    /// a partial mapping is never cached as complete.
    async fn fetch(&self, beacon: Address) -> Result<Vec<Address>, String> {
        let mut out: Vec<Address> = Vec::new();
        let mut offset = 0usize;
        for _ in 0..MAX_PAGES {
            let url = markets_url(&self.base_url, beacon, PAGE_LIMIT, offset);
            let resp = self
                .client
                .get(&url)
                .header("X-API-Key", &self.api_key)
                .send()
                .await
                .map_err(|e| format!("request failed: {e}"))?;

            let status = resp.status();
            if !status.is_success() {
                // Distinguish auth (config) errors from transient ones for alerting.
                let metric = if status.as_u16() == 401 || status.as_u16() == 403 {
                    "MappingAuthError"
                } else {
                    "MappingFetchError"
                };
                return Err(format!("bot-api returned {status} [{metric}]"));
            }

            let body = resp
                .text()
                .await
                .map_err(|e| format!("read body failed: {e}"))?;
            let (perps, has_more) = parse_markets_page(&body)?;
            out.extend(perps);
            if !has_more {
                break;
            }
            offset += PAGE_LIMIT;
        }
        dedup_preserving_order(&mut out);
        Ok(out)
    }
}

// ---- pure helpers (unit-tested from tests/unit_tests/touch_tests.rs) ----

/// Build the `GET /markets` URL. No trailing slash on `/markets`: FastAPI
/// 307-redirects `/markets/` and some HTTP clients choke on the redirect.
pub fn markets_url(base: &str, beacon: Address, limit: usize, offset: usize) -> String {
    format!("{base}/markets?beacon={beacon:#x}&limit={limit}&offset={offset}")
}

/// Parse the perp addresses from one `/markets` page body. Unparseable
/// `perp_address` values are skipped (logged), not fatal.
pub fn parse_perp_addresses_from_json(body: &str) -> Result<Vec<Address>, String> {
    Ok(parse_markets_page(body)?.0)
}

fn parse_markets_page(body: &str) -> Result<(Vec<Address>, bool), String> {
    let page: MarketsPage =
        serde_json::from_str(body).map_err(|e| format!("decode failed: {e}"))?;
    let perps = page
        .items
        .iter()
        .filter_map(|it| match Address::from_str(it.perp_address.trim()) {
            Ok(addr) => Some(addr),
            Err(e) => {
                tracing::warn!(
                    target: "touch",
                    perp_address = %it.perp_address,
                    error = %e,
                    "skipping unparseable perp_address from bot-api"
                );
                None
            }
        })
        .collect();
    Ok((perps, page.has_more))
}

/// A cache entry is fresh within `ttl` normally, or within the shorter
/// `empty_ttl` when it resolved to no perps (so a newly-created market is
/// picked up quickly without hammering bot-api).
pub fn entry_is_fresh(
    fetched_at: Instant,
    is_empty: bool,
    ttl: Duration,
    empty_ttl: Duration,
    now: Instant,
) -> bool {
    let age = now.saturating_duration_since(fetched_at);
    if is_empty { age < empty_ttl } else { age < ttl }
}

/// Remove duplicate addresses while preserving first-seen order.
pub fn dedup_preserving_order(addrs: &mut Vec<Address>) {
    let mut seen = std::collections::HashSet::new();
    addrs.retain(|a| seen.insert(*a));
}
