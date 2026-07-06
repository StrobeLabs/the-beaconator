//! Integration tests for the wallet-pool balance sweep.
//!
//! Covers the Multicall3 aggregation path (one aggregate3 eth_call for the
//! whole pool), its per-wallet failure tolerance, and the automatic fallback
//! to per-wallet reads when no usable Multicall3 deployment exists.
//!
//! Requires compiled mock artifacts: `cd tests/contracts && forge build`.

use alloy::network::EthereumWallet;
use alloy::primitives::{Address, U256};
use alloy::providers::ProviderBuilder;
use alloy::sol;
use std::str::FromStr;
use std::sync::Arc;
use the_beaconator::ReadOnlyProvider;
use the_beaconator::services::wallet::BalanceTracker;

use crate::test_utils::{AnvilManager, deploy_contract, load_contract_bytecode};

sol! {
    #[sol(rpc)]
    interface IMockUSDC {
        function mint(address to, uint256 amount) external;
        function balanceOf(address account) external view returns (uint256);
    }
}

struct SweepFixture {
    _anvil: AnvilManager,
    read_provider: Arc<ReadOnlyProvider>,
    usdc: Address,
    multicall3: Address,
    wallets: Vec<Address>,
}

/// Deploy MockMulticall3 + MockUSDC to a fresh Anvil instance and mint
/// distinct USDC balances to the first two Anvil accounts.
async fn setup_sweep_fixture() -> SweepFixture {
    let anvil = AnvilManager::new().await;

    let signer = anvil.deployer_signer();
    let wallet = EthereumWallet::from(signer);
    let deploy_provider = Arc::new(
        ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(anvil.rpc_url().parse().expect("valid anvil url")),
    );

    let multicall3 = deploy_contract(&deploy_provider, load_contract_bytecode("MockMulticall3"))
        .await
        .expect("deploy MockMulticall3");
    let usdc = deploy_contract(&deploy_provider, load_contract_bytecode("MockUSDC"))
        .await
        .expect("deploy MockUSDC");

    let wallets = vec![anvil.deployer_account(), anvil.get_signer(1).address()];

    let usdc_contract = IMockUSDC::new(usdc, &*deploy_provider);
    for (i, &w) in wallets.iter().enumerate() {
        usdc_contract
            .mint(w, U256::from((i as u64 + 1) * 25_000_000)) // 25 / 50 USDC
            .send()
            .await
            .expect("send mint")
            .get_receipt()
            .await
            .expect("mint receipt");
    }

    let read_provider: Arc<ReadOnlyProvider> = Arc::new(
        ProviderBuilder::new().connect_http(anvil.rpc_url().parse().expect("valid anvil url")),
    );

    SweepFixture {
        _anvil: anvil,
        read_provider,
        usdc,
        multicall3,
        wallets,
    }
}

#[tokio::test]
async fn test_multicall_sweep_populates_balances() {
    let fixture = setup_sweep_fixture().await;

    let tracker = BalanceTracker::new(
        fixture.read_provider.clone(),
        fixture.usdc,
        Some(fixture.multicall3),
    );
    tracker.refresh(&fixture.wallets).await;

    for (i, wallet) in fixture.wallets.iter().enumerate() {
        let cached = tracker
            .get(wallet)
            .unwrap_or_else(|| panic!("no cached balances for wallet {wallet}"));
        assert!(cached.eth > U256::ZERO, "ETH balance should be non-zero");
        assert_eq!(
            cached.usdc,
            U256::from((i as u64 + 1) * 25_000_000),
            "cached USDC should match minted amount"
        );
    }
}

#[tokio::test]
async fn test_multicall_sweep_matches_direct_reads() {
    let fixture = setup_sweep_fixture().await;

    let multicall_tracker = BalanceTracker::new(
        fixture.read_provider.clone(),
        fixture.usdc,
        Some(fixture.multicall3),
    );
    let sequential_tracker = BalanceTracker::new(fixture.read_provider.clone(), fixture.usdc, None);

    multicall_tracker.refresh(&fixture.wallets).await;
    sequential_tracker.refresh(&fixture.wallets).await;

    for wallet in &fixture.wallets {
        let via_multicall = multicall_tracker.get(wallet).expect("multicall cache");
        let via_sequential = sequential_tracker.get(wallet).expect("sequential cache");
        assert_eq!(via_multicall.eth, via_sequential.eth);
        assert_eq!(via_multicall.usdc, via_sequential.usdc);
    }
}

#[tokio::test]
async fn test_sweep_falls_back_when_multicall_has_no_code() {
    let fixture = setup_sweep_fixture().await;

    // Canonical Multicall3 address: nothing is deployed there on bare Anvil,
    // so the aggregated call fails and the sweep must fall back to
    // per-wallet reads without losing any balances.
    let missing_multicall =
        Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap();
    let tracker = BalanceTracker::new(
        fixture.read_provider.clone(),
        fixture.usdc,
        Some(missing_multicall),
    );
    tracker.refresh(&fixture.wallets).await;

    for (i, wallet) in fixture.wallets.iter().enumerate() {
        let cached = tracker
            .get(wallet)
            .unwrap_or_else(|| panic!("fallback should cache balances for {wallet}"));
        assert_eq!(cached.usdc, U256::from((i as u64 + 1) * 25_000_000));
    }
}

#[tokio::test]
async fn test_multicall_sweep_skips_undecodable_wallet_reads() {
    let fixture = setup_sweep_fixture().await;

    // Point USDC at an address with no code: the inner balanceOf calls
    // "succeed" with empty return data, which must be skipped per wallet
    // (warn-and-skip) rather than poisoning the sweep or caching garbage.
    let no_code_usdc = Address::from_str("0x00000000000000000000000000000000DeaDBeef").unwrap();
    let tracker = BalanceTracker::new(
        fixture.read_provider.clone(),
        no_code_usdc,
        Some(fixture.multicall3),
    );
    tracker.refresh(&fixture.wallets).await;

    for wallet in &fixture.wallets {
        assert!(
            tracker.get(wallet).is_none(),
            "undecodable balance reads must not be cached"
        );
    }
}
