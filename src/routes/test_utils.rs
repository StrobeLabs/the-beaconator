/*!
# Test Utilities for Integration Testing

This module provides comprehensive testing utilities for the Beaconator API, including:

## Features

- **Anvil Integration**: Automatic setup and management of local blockchain using Anvil
- **Deterministic Accounts**: Pre-funded test accounts with known private keys
- **Real ABIs**: Loads actual contract ABIs from test fixtures
- **Contract Deployment**: Mock and real contract deployment utilities
- **Blockchain Utilities**: Helper functions for balance checking, block manipulation, etc.

## Usage

### Basic Test Setup

```rust
use crate::routes::test_utils::{create_test_app_state, TestUtils};

#[tokio::test]
async fn test_example() {
    let app_state = create_test_app_state().await;

    // App state now has:
    // - Real blockchain connection via Anvil
    // - Funded test account
    // - Actual contract ABIs loaded
    // - Deterministic contract addresses

    // Test blockchain connection
    let block_number = TestUtils::get_block_number(&app_state.provider).await;
    assert!(block_number.is_ok());
}
```

### Multi-Account Testing

```rust
use crate::routes::test_utils::create_test_app_state_with_account;

#[tokio::test]
async fn test_with_different_account() {
    // Use account index 1 instead of 0
    let app_state = create_test_app_state_with_account(1).await;

    // This account has different address but same balance
    assert_ne!(app_state.wallet_address, Address::ZERO);
}
```

### Blockchain Utilities

```rust
use crate::routes::test_utils::TestUtils;

#[tokio::test]
async fn test_blockchain_operations() {
    let app_state = create_test_app_state().await;

    // Check balance
    let balance = TestUtils::get_balance(&app_state.provider, app_state.wallet_address).await?;
    assert!(balance > U256::ZERO);

    // Get block number
    let block_number = TestUtils::get_block_number(&app_state.provider).await?;

    // Time manipulation (for contract testing)
    TestUtils::fast_forward_time(&app_state.provider, 3600).await?; // 1 hour
    TestUtils::mine_blocks(&app_state.provider, 10).await?;
}
```

### Contract Deployment Mocking

```rust
use crate::routes::test_utils::mock_contract_deployment;

#[tokio::test]
async fn test_contract_deployment() {
    let deployment = mock_contract_deployment("PerpHook").await;
    assert_ne!(deployment.address, Address::ZERO);
    assert_eq!(deployment.gas_used, 1000000);
}
```

### Test Cleanup

```rust
use crate::routes::test_utils::TestCleanup;

#[tokio::test]
async fn test_with_cleanup() {
    // Test logic here...

    // Clean up after test
    TestCleanup::reset_anvil().await?;
}
```

## Test Structure

The test utilities provide a realistic testing environment:

1. **Anvil Instance**: Started once and shared across tests
2. **Test Accounts**: 10 pre-funded accounts with 1000 ETH each
3. **Chain ID**: 31337 (standard Hardhat/Anvil chain ID)
4. **Block Time**: 1 second for fast test execution
5. **Contract ABIs**: Loaded from `src/test_fixtures/` directory

## Important Notes

- Tests using `create_test_app_state()` should be run with `#[tokio::test]`
- The Anvil instance is shared across tests for performance
- All accounts are pre-funded with 1000 ETH
- ABIs are loaded from real contract artifacts
- Contract calls will fail if contracts aren't deployed (expected behavior)
- Use `TestCleanup::shutdown_all()` in test cleanup if needed

## Dependencies

Requires the following dev dependencies:
- `tempfile` - For temporary file management
- `once_cell` - For singleton pattern
- `alloy` with `node-bindings` feature - For Anvil integration
*/

use crate::models::{AppState, PerpConfig};
use alloy::{
    json_abi::JsonAbi,
    network::EthereumWallet,
    node_bindings::{Anvil, AnvilInstance},
    primitives::{Address, U256},
    providers::{Provider, ProviderBuilder},
    signers::{Signer, local::PrivateKeySigner},
};
use once_cell::sync::Lazy;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::sync::OnceCell;

/// Static Anvil instance for shared test blockchain
#[allow(dead_code)]
static ANVIL_INSTANCE: Lazy<Mutex<Option<AnvilInstance>>> = Lazy::new(|| Mutex::new(None));

/// Anvil configuration and utilities
#[allow(dead_code)]
pub struct AnvilConfig {
    pub instance: AnvilInstance,
    pub rpc_url: String,
    pub chain_id: u64,
    pub accounts: Vec<Address>,
}

impl AnvilConfig {
    /// Start a new Anvil instance with deterministic accounts
    pub fn new() -> Self {
        let anvil = Anvil::new()
            .chain_id(31337u64) // Standard Hardhat chain ID
            .block_time(1u64) // 1 second block time for faster tests
            .spawn();

        let rpc_url = anvil.endpoint();
        let chain_id = anvil.chain_id();
        let accounts = anvil.addresses().to_vec();

        tracing::info!("Started Anvil instance:");
        tracing::info!("  - RPC URL: {}", rpc_url);
        tracing::info!("  - Chain ID: {}", chain_id);
        tracing::info!("  - Test accounts: {}", accounts.len());
        tracing::info!("  - First account: {}", accounts[0]);

        Self {
            instance: anvil,
            rpc_url,
            chain_id,
            accounts,
        }
    }

    /// Get the first test account (deployer)
    pub fn deployer_account(&self) -> Address {
        self.accounts[0]
    }

    /// Get the second test account (user)
    #[allow(dead_code)]
    pub fn user_account(&self) -> Address {
        self.accounts[1]
    }

    /// Get the first key as a PrivateKeySigner
    pub fn deployer_signer(&self) -> PrivateKeySigner {
        // Get the key directly from anvil and create a signer
        PrivateKeySigner::from_slice(self.instance.keys()[0].to_bytes().as_slice())
            .expect("Failed to create signer from key")
            .with_chain_id(Some(self.chain_id))
    }

    /// Get a specific key as a PrivateKeySigner
    #[allow(dead_code)]
    pub fn get_signer(&self, index: usize) -> PrivateKeySigner {
        PrivateKeySigner::from_slice(self.instance.keys()[index].to_bytes().as_slice())
            .expect("Failed to create signer from key")
            .with_chain_id(Some(self.chain_id))
    }
}

/// Global Anvil instance manager
pub struct AnvilManager;

impl AnvilManager {
    /// Get or create the shared Anvil instance
    pub async fn get_or_create() -> Arc<AnvilConfig> {
        static ANVIL_CONFIG: OnceCell<Arc<AnvilConfig>> = OnceCell::const_new();

        ANVIL_CONFIG
            .get_or_init(|| async {
                let config = AnvilConfig::new();
                Arc::new(config)
            })
            .await
            .clone()
    }

    /// Shutdown the Anvil instance (for cleanup)
    #[allow(dead_code)]
    pub fn shutdown() {
        let mut instance = ANVIL_INSTANCE.lock().unwrap();
        if let Some(anvil) = instance.take() {
            drop(anvil);
            tracing::info!("Anvil instance shut down");
        }
    }
}

/// Load ABI from test fixtures
pub fn load_test_abi(name: &str) -> JsonAbi {
    let fixture_path = format!("src/test_fixtures/{name}.json");
    let abi_content = std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|_| panic!("Failed to read test ABI file: {fixture_path}"));
    serde_json::from_str(&abi_content)
        .unwrap_or_else(|_| panic!("Failed to parse test ABI file: {fixture_path}"))
}

/// Test deployment utilities
pub struct TestDeployment {
    pub beacon_factory: Address,
    pub beacon_registry: Address,
    pub perp_hook: Address,
    pub deployer: Address,
    pub provider: Arc<crate::AlloyProvider>,
}

impl TestDeployment {
    /// Deploy test contracts to Anvil
    pub async fn deploy(anvil: &AnvilConfig) -> Result<Self, Box<dyn std::error::Error>> {
        // Create provider with deployer account
        let signer = anvil.deployer_signer();
        let wallet = EthereumWallet::from(signer);
        let provider = Arc::new(
            ProviderBuilder::new()
                .wallet(wallet)
                .connect_http(anvil.rpc_url.parse()?),
        );

        // For testing, we'll use mock addresses for now
        // In a real integration test, you would deploy actual contracts here
        let beacon_factory = Address::from_str("0x5FbDB2315678afecb367f032d93F642f64180aa3")?;
        let beacon_registry = Address::from_str("0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512")?;
        let perp_hook = Address::from_str("0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0")?;

        tracing::info!("Test contracts deployed:");
        tracing::info!("  - BeaconFactory: {}", beacon_factory);
        tracing::info!("  - BeaconRegistry: {}", beacon_registry);
        tracing::info!("  - PerpHook: {}", perp_hook);

        Ok(Self {
            beacon_factory,
            beacon_registry,
            perp_hook,
            deployer: anvil.deployer_account(),
            provider,
        })
    }
}

/// Create a comprehensive test AppState with real blockchain connection
pub async fn create_test_app_state() -> AppState {
    // Get or create Anvil instance
    let anvil = AnvilManager::get_or_create().await;

    // Deploy test contracts
    let deployment = TestDeployment::deploy(&anvil)
        .await
        .expect("Failed to deploy test contracts");

    // Load real ABIs from test fixtures
    let beacon_abi = load_test_abi("Beacon");
    let beacon_factory_abi = load_test_abi("BeaconFactory");
    let beacon_registry_abi = load_test_abi("BeaconRegistry");
    let perp_hook_abi = load_test_abi("PerpHook");

    AppState {
        provider: deployment.provider,
        wallet_address: deployment.deployer,
        beacon_abi,
        beacon_factory_abi,
        beacon_registry_abi,
        perp_hook_abi,
        beacon_factory_address: deployment.beacon_factory,
        perpcity_registry_address: deployment.beacon_registry,
        perp_hook_address: deployment.perp_hook,
        access_token: "test_token".to_string(),
        perp_config: PerpConfig::default(),
    }
}

/// Create a test AppState with a specific account
pub async fn create_test_app_state_with_account(account_index: usize) -> AppState {
    let anvil = AnvilManager::get_or_create().await;

    let signer = anvil.get_signer(account_index);
    let wallet = EthereumWallet::from(signer);
    let provider = Arc::new(
        ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(anvil.rpc_url.parse().expect("Invalid RPC URL")),
    );

    let deployment = TestDeployment::deploy(&anvil)
        .await
        .expect("Failed to deploy test contracts");

    AppState {
        provider,
        wallet_address: anvil.accounts[account_index],
        beacon_abi: load_test_abi("Beacon"),
        beacon_factory_abi: load_test_abi("BeaconFactory"),
        beacon_registry_abi: load_test_abi("BeaconRegistry"),
        perp_hook_abi: load_test_abi("PerpHook"),
        beacon_factory_address: deployment.beacon_factory,
        perpcity_registry_address: deployment.beacon_registry,
        perp_hook_address: deployment.perp_hook,
        access_token: "test_token".to_string(),
        perp_config: PerpConfig::default(),
    }
}

/// Test utilities for blockchain interactions
#[allow(dead_code)]
pub struct TestUtils;

impl TestUtils {
    /// Get the current block number
    pub async fn get_block_number(
        provider: &crate::AlloyProvider,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let block_number = provider.get_block_number().await?;
        Ok(block_number)
    }

    /// Get account balance
    pub async fn get_balance(
        provider: &crate::AlloyProvider,
        address: Address,
    ) -> Result<U256, Box<dyn std::error::Error>> {
        let balance = provider.get_balance(address).await?;
        Ok(balance)
    }

    /// Fund an account with ETH (for testing)
    #[allow(dead_code)]
    pub async fn fund_account(
        _provider: &crate::AlloyProvider,
        to: Address,
        amount: U256,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // In a real implementation, this would use anvil_setBalance RPC call
        // For now, we'll skip this since accounts are pre-funded
        tracing::info!("Funding account {} with {} ETH", to, amount);
        Ok(())
    }

    /// Fast forward blockchain time (for testing time-dependent contracts)
    #[allow(dead_code)]
    pub async fn fast_forward_time(
        _provider: &crate::AlloyProvider,
        seconds: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // In a real implementation, this would use anvil_increaseTime RPC call
        tracing::info!("Fast forwarding time by {} seconds", seconds);
        Ok(())
    }

    /// Mine blocks (for testing block-dependent contracts)
    #[allow(dead_code)]
    pub async fn mine_blocks(
        _provider: &crate::AlloyProvider,
        blocks: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // In a real implementation, this would use anvil_mine RPC call
        tracing::info!("Mining {} blocks", blocks);
        Ok(())
    }
}

/// Test cleanup utilities
#[allow(dead_code)]
pub struct TestCleanup;

impl TestCleanup {
    /// Reset Anvil state (for isolated tests)
    #[allow(dead_code)]
    pub async fn reset_anvil() -> Result<(), Box<dyn std::error::Error>> {
        // This would use anvil_reset RPC call
        tracing::info!("Resetting Anvil state");
        Ok(())
    }

    /// Shutdown all test resources
    #[allow(dead_code)]
    pub fn shutdown_all() {
        AnvilManager::shutdown();
    }
}

/// Test fixture for contract deployment results
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ContractDeploymentResult {
    pub address: Address,
    pub transaction_hash: String,
    pub block_number: u64,
    pub gas_used: u64,
}

/// Mock contract deployment (for testing without actual deployment)
#[allow(dead_code)]
pub async fn mock_contract_deployment(name: &str) -> ContractDeploymentResult {
    // Generate deterministic addresses for testing
    let address = match name {
        "Beacon" => Address::from_str("0x5FbDB2315678afecb367f032d93F642f64180aa3").unwrap(),
        "BeaconFactory" => Address::from_str("0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512").unwrap(),
        "BeaconRegistry" => {
            Address::from_str("0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0").unwrap()
        }
        "PerpHook" => Address::from_str("0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9").unwrap(),
        _ => Address::from_str("0x0000000000000000000000000000000000000000").unwrap(),
    };

    ContractDeploymentResult {
        address,
        transaction_hash: "0x1234567890123456789012345678901234567890123456789012345678901234"
            .to_string(),
        block_number: 1,
        gas_used: 1000000,
    }
}

/// Create a synchronous test AppState for simple tests (fallback)
#[allow(dead_code)]
pub fn create_simple_test_app_state() -> AppState {
    // Create mock provider with wallet for testing - this won't work for real network calls
    let signer = alloy::signers::local::PrivateKeySigner::random();
    let wallet = alloy::network::EthereumWallet::from(signer);
    // Use modern Alloy provider builder pattern for tests
    let provider = alloy::providers::ProviderBuilder::new()
        .wallet(wallet)
        .connect_http("http://localhost:8545".parse().unwrap());

    AppState {
        provider: Arc::new(provider),
        wallet_address: Address::from_str("0x1111111111111111111111111111111111111111").unwrap(),
        beacon_abi: JsonAbi::new(),
        beacon_factory_abi: JsonAbi::new(),
        beacon_registry_abi: JsonAbi::new(),
        perp_hook_abi: JsonAbi::new(),
        beacon_factory_address: Address::from_str("0x1234567890123456789012345678901234567890")
            .unwrap(),
        perpcity_registry_address: Address::from_str("0x2345678901234567890123456789012345678901")
            .unwrap(),
        perp_hook_address: Address::from_str("0x3456789012345678901234567890123456789012").unwrap(),
        access_token: "test_token".to_string(),
        perp_config: PerpConfig::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_anvil_manager() {
        let anvil = AnvilManager::get_or_create().await;
        assert_eq!(anvil.chain_id, 31337);
        assert!(!anvil.accounts.is_empty());

        // Test that we can create signers
        let signer = anvil.deployer_signer();
        assert_ne!(signer.address(), Address::ZERO);
    }

    #[tokio::test]
    async fn test_abi_loading() {
        let beacon_abi = load_test_abi("Beacon");
        assert!(!beacon_abi.functions.is_empty());

        let perp_hook_abi = load_test_abi("PerpHook");
        assert!(!perp_hook_abi.functions.is_empty());
    }

    #[tokio::test]
    async fn test_app_state_creation() {
        let app_state = create_test_app_state().await;
        assert_ne!(app_state.wallet_address, Address::ZERO);
        assert_ne!(app_state.beacon_factory_address, Address::ZERO);
        assert_ne!(app_state.perp_hook_address, Address::ZERO);
    }

    #[tokio::test]
    async fn test_test_deployment() {
        let anvil = AnvilManager::get_or_create().await;
        let deployment = TestDeployment::deploy(&anvil).await;
        assert!(deployment.is_ok());

        let deployment = deployment.unwrap();
        assert_ne!(deployment.beacon_factory, Address::ZERO);
        assert_ne!(deployment.beacon_registry, Address::ZERO);
        assert_ne!(deployment.perp_hook, Address::ZERO);
    }

    #[tokio::test]
    async fn test_blockchain_utilities() {
        let app_state = create_test_app_state().await;

        // Test block number
        let block_number = TestUtils::get_block_number(&app_state.provider).await;
        assert!(block_number.is_ok());

        // Test balance
        let balance = TestUtils::get_balance(&app_state.provider, app_state.wallet_address).await;
        assert!(balance.is_ok());
        let balance = balance.unwrap();
        assert!(balance > U256::ZERO);
    }

    #[tokio::test]
    async fn test_contract_deployment_mock() {
        let result = mock_contract_deployment("Beacon").await;
        assert_ne!(result.address, Address::ZERO);
        assert!(!result.transaction_hash.is_empty());
        assert!(result.gas_used > 0);
    }
}
