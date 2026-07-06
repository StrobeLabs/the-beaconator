//! Proactive balance tracking for the gas-payer wallet pool
//!
//! On 2026-06-30 a single drained testnet pool wallet froze the entire beacon
//! update fleet: selection picked purely by lock status (never balance), so
//! ~90% of acquisitions kept landing on the empty wallet and every send
//! failed with no retry. `BalanceTracker` closes that gap two ways:
//!   - a background sweep periodically refreshes cached ETH/USDC balances and
//!     emits a CloudWatch metric per wallet, plus a warning log when a wallet
//!     drops below the ETH floor (visibility before it's a fire);
//!   - `WalletManager` selection consults the cache to skip a wallet that is
//!     already known to be under the floor, without ever blocking the
//!     acquisition hot path on a fresh RPC call.
//!
//! The cache can be stale (up to one sweep interval): callers that need a
//! guarantee (funding routes) still do a fresh on-chain check after
//! acquisition. This tracker is a proactive optimization, not a source of
//! truth.
//!
//! When a Multicall3 address is configured (`MULTICALL3_ADDRESS`, same config
//! the beacon batch path uses), the sweep aggregates all per-wallet reads
//! (`getEthBalance` + `USDC.balanceOf`) into one `aggregate3` eth_call per
//! interval instead of 2 RPC calls per wallet. A failed multicall (e.g. no
//! contract at the address on a bare test chain) falls back to per-wallet
//! reads, so the sweep never regresses below the pre-multicall behavior.

use alloy::primitives::{Address, U256};
use alloy::providers::Provider;
use alloy::sol_types::{SolCall, SolValue};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::ReadOnlyProvider;
use crate::routes::{IERC20, IMulticall3};

/// Default ETH floor (wei) below which a pool wallet is flagged and skipped
/// by proactive selection: 0.0005 ETH.
const DEFAULT_MIN_ETH_WEI: u128 = 500_000_000_000_000;
/// Default interval between balance sweeps.
const DEFAULT_SWEEP_SECS: u64 = 60;

/// Cached ETH + USDC balances for one pool wallet.
#[derive(Debug, Clone, Copy)]
pub struct WalletBalances {
    pub eth: U256,
    pub usdc: U256,
    pub fetched_at: Instant,
}

/// Periodically refreshed balance cache for the gas-payer pool.
pub struct BalanceTracker {
    provider: Arc<ReadOnlyProvider>,
    usdc: Address,
    multicall3: Option<Address>,
    eth_floor: U256,
    balances: RwLock<HashMap<Address, WalletBalances>>,
}

impl BalanceTracker {
    /// Create a tracker with the ETH floor read from `WALLET_MIN_ETH_WEI`
    /// (falls back to 0.0005 ETH if unset or unparseable). `multicall3` is
    /// the configured Multicall3 address; `None` keeps per-wallet reads.
    pub fn new(
        provider: Arc<ReadOnlyProvider>,
        usdc: Address,
        multicall3: Option<Address>,
    ) -> Self {
        Self {
            provider,
            usdc,
            multicall3,
            eth_floor: Self::eth_floor_from_env(),
            balances: RwLock::new(HashMap::new()),
        }
    }

    fn eth_floor_from_env() -> U256 {
        std::env::var("WALLET_MIN_ETH_WEI")
            .ok()
            .and_then(|v| v.trim().parse::<u128>().ok())
            .map(U256::from)
            .unwrap_or_else(|| U256::from(DEFAULT_MIN_ETH_WEI))
    }

    /// Balance sweep interval read from `WALLET_BALANCE_SWEEP_SECS` (falls
    /// back to 60s if unset or unparseable).
    pub fn sweep_interval_from_env() -> Duration {
        let secs = std::env::var("WALLET_BALANCE_SWEEP_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_SWEEP_SECS);
        Duration::from_secs(secs)
    }

    /// The configured ETH floor (wei).
    pub fn eth_floor(&self) -> U256 {
        self.eth_floor
    }

    /// Refresh ETH + USDC balances for the given wallets. Best-effort per
    /// wallet: a failed fetch for one address is logged and skipped, it does
    /// not abort the rest of the sweep.
    ///
    /// Uses one Multicall3 `aggregate3` eth_call for the whole pool when a
    /// Multicall3 address is configured, with automatic fallback to the
    /// per-wallet read path if the aggregated call fails as a whole.
    pub async fn refresh(&self, wallets: &[Address]) {
        if wallets.is_empty() {
            return;
        }

        if let Some(multicall3) = self.multicall3 {
            match self.refresh_via_multicall(multicall3, wallets).await {
                Ok(()) => return,
                Err(e) => {
                    tracing::warn!(
                        "Balance sweep multicall via {multicall3} failed ({e}); \
                         falling back to per-wallet reads"
                    );
                }
            }
        }

        self.refresh_sequential(wallets).await;
    }

    /// Fetch all wallet balances in a single Multicall3 `aggregate3` call:
    /// per wallet, `getEthBalance(wallet)` (a Multicall3 built-in) plus
    /// `USDC.balanceOf(wallet)`, both with `allowFailure: true` so one bad
    /// wallet read doesn't poison the sweep.
    async fn refresh_via_multicall(
        &self,
        multicall3: Address,
        wallets: &[Address],
    ) -> Result<(), String> {
        let mut calls = Vec::with_capacity(wallets.len() * 2);
        for &address in wallets {
            calls.push(IMulticall3::Call3 {
                target: multicall3,
                allowFailure: true,
                callData: IMulticall3::getEthBalanceCall { addr: address }
                    .abi_encode()
                    .into(),
            });
            calls.push(IMulticall3::Call3 {
                target: self.usdc,
                allowFailure: true,
                callData: IERC20::balanceOfCall { account: address }
                    .abi_encode()
                    .into(),
            });
        }

        let contract = IMulticall3::new(multicall3, &*self.provider);
        let results = contract
            .aggregate3(calls)
            .call()
            .await
            .map_err(|e| e.to_string())?;

        if results.len() != wallets.len() * 2 {
            return Err(format!(
                "expected {} multicall results, got {}",
                wallets.len() * 2,
                results.len()
            ));
        }

        for (&address, pair) in wallets.iter().zip(results.chunks_exact(2)) {
            let eth = pair[0]
                .success
                .then(|| U256::abi_decode(&pair[0].returnData).ok())
                .flatten();
            let usdc = pair[1]
                .success
                .then(|| U256::abi_decode(&pair[1].returnData).ok())
                .flatten();

            match (eth, usdc) {
                (Some(eth), Some(usdc)) => self.store(
                    address,
                    WalletBalances {
                        eth,
                        usdc,
                        fetched_at: Instant::now(),
                    },
                ),
                _ => {
                    tracing::warn!(
                        "Failed to refresh balances for wallet {address} in multicall sweep"
                    );
                }
            }
        }

        Ok(())
    }

    /// Per-wallet read path: 2 RPC calls per wallet. Used when no Multicall3
    /// address is configured or the aggregated call failed.
    async fn refresh_sequential(&self, wallets: &[Address]) {
        let usdc_contract = IERC20::new(self.usdc, &*self.provider);

        for &address in wallets {
            let eth = match self.provider.get_balance(address).await {
                Ok(bal) => bal,
                Err(e) => {
                    tracing::warn!("Failed to refresh ETH balance for wallet {address}: {e}");
                    continue;
                }
            };

            let usdc = match usdc_contract.balanceOf(address).call().await {
                Ok(bal) => bal,
                Err(e) => {
                    tracing::warn!("Failed to refresh USDC balance for wallet {address}: {e}");
                    continue;
                }
            };

            self.store(
                address,
                WalletBalances {
                    eth,
                    usdc,
                    fetched_at: Instant::now(),
                },
            );
        }
    }

    fn store(&self, address: Address, entry: WalletBalances) {
        match self.balances.write() {
            Ok(mut map) => {
                map.insert(address, entry);
            }
            Err(e) => {
                tracing::error!("Wallet balance cache lock poisoned: {e}");
            }
        }
    }

    /// Get the cached balances for a wallet, if any have been fetched yet.
    pub fn get(&self, address: &Address) -> Option<WalletBalances> {
        match self.balances.read() {
            Ok(map) => map.get(address).copied(),
            Err(e) => {
                tracing::error!("Wallet balance cache lock poisoned: {e}");
                None
            }
        }
    }

    /// Spawn a background task that refreshes balances every `interval` and,
    /// for each wallet, emits CloudWatch metrics (best-effort, silent
    /// locally) and — for any wallet under the ETH floor — logs a warning so
    /// an operator can top it up before it freezes selection entirely.
    pub fn spawn_sweep(
        self: Arc<Self>,
        manager_addresses: Vec<Address>,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let metrics = CloudWatchMetrics::new().await;
            loop {
                self.refresh(&manager_addresses).await;

                for &address in &manager_addresses {
                    if let Some(bal) = self.get(&address) {
                        if bal.eth < self.eth_floor {
                            tracing::warn!(
                                wallet = %address,
                                eth_balance = %bal.eth,
                                "pool wallet below ETH floor - fund it"
                            );
                        }
                        metrics
                            .put_wallet_balances(address, bal.eth, bal.usdc)
                            .await;
                    }
                }

                tokio::time::sleep(interval).await;
            }
        })
    }
}

/// Best-effort CloudWatch PutMetricData for pool wallet balances.
///
/// Local dev has no AWS credentials, so publish failures are expected there
/// and must never spam logs — they're logged at `debug` only. Alarms on
/// these metrics are configured separately in the SST app, not here (see
/// `sst.config.ts`); this only emits the raw data points.
struct CloudWatchMetrics {
    client: aws_sdk_cloudwatch::Client,
    environment: String,
}

impl CloudWatchMetrics {
    async fn new() -> Self {
        let environment = std::env::var("ENV").unwrap_or_else(|_| "unknown".to_string());
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = aws_sdk_cloudwatch::Client::new(&config);
        Self {
            client,
            environment,
        }
    }

    async fn put_wallet_balances(&self, address: Address, eth: U256, usdc: U256) {
        use aws_sdk_cloudwatch::types::{Dimension, MetricDatum, StandardUnit};

        let env_dim = Dimension::builder()
            .name("Environment")
            .value(self.environment.clone())
            .build();
        let wallet_dim = Dimension::builder()
            .name("WalletAddress")
            .value(address.to_checksum(None))
            .build();

        let eth_datum = MetricDatum::builder()
            .metric_name("WalletEthBalance")
            .unit(StandardUnit::None)
            .value(wei_to_f64(eth, 1e18))
            .dimensions(env_dim.clone())
            .dimensions(wallet_dim.clone())
            .build();

        let usdc_datum = MetricDatum::builder()
            .metric_name("WalletUsdcBalance")
            .unit(StandardUnit::None)
            .value(wei_to_f64(usdc, 1e6))
            .dimensions(env_dim)
            .dimensions(wallet_dim)
            .build();

        if let Err(e) = self
            .client
            .put_metric_data()
            .namespace("PerpCity")
            .metric_data(eth_datum)
            .metric_data(usdc_datum)
            .send()
            .await
        {
            // Local dev has no AWS credentials — this is the expected path there,
            // so debug only. See sst.config.ts follow-up for the required
            // cloudwatch:PutMetricData task-role grant in deployed environments.
            tracing::debug!("Failed to publish wallet balance metrics for {address}: {e}");
        }
    }
}

/// Scale a token-unit `U256` amount (wei, USDC micros, ...) down to a human
/// `f64` for CloudWatch. Balances comfortably fit in `u128`; saturate rather
/// than panic in the unlikely event they don't.
fn wei_to_f64(value: U256, scale: f64) -> f64 {
    let capped: u128 = u128::try_from(&value).unwrap_or(u128::MAX);
    capped as f64 / scale
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn test_address(byte: u8) -> Address {
        Address::from([byte; 20])
    }

    #[test]
    fn test_wei_to_f64_eth() {
        let one_eth = U256::from(1_000_000_000_000_000_000u128);
        assert_eq!(wei_to_f64(one_eth, 1e18), 1.0);
    }

    #[test]
    fn test_wei_to_f64_usdc() {
        let hundred_usdc = U256::from(100_000_000u128);
        assert_eq!(wei_to_f64(hundred_usdc, 1e6), 100.0);
    }

    #[test]
    fn test_wei_to_f64_zero() {
        assert_eq!(wei_to_f64(U256::ZERO, 1e18), 0.0);
    }

    #[test]
    fn test_get_returns_none_before_refresh() {
        let provider = std::sync::Arc::new(
            alloy::providers::ProviderBuilder::new()
                .connect_http("http://127.0.0.1:1".parse().unwrap()),
        );
        let usdc = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
        let tracker = BalanceTracker::new(provider, usdc, None);

        assert!(tracker.get(&test_address(0x01)).is_none());
    }

    #[test]
    #[serial_test::serial]
    fn test_default_eth_floor_when_env_unset() {
        // SAFETY: #[serial] guarantees no concurrent env access from other tests.
        unsafe {
            std::env::remove_var("WALLET_MIN_ETH_WEI");
        }
        let provider = std::sync::Arc::new(
            alloy::providers::ProviderBuilder::new()
                .connect_http("http://127.0.0.1:1".parse().unwrap()),
        );
        let usdc = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();
        let tracker = BalanceTracker::new(provider, usdc, None);

        assert_eq!(tracker.eth_floor(), U256::from(DEFAULT_MIN_ETH_WEI));
    }
}
