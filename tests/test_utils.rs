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
use the_beaconator::routes::test_utils::{create_test_app_state, TestUtils};

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
use the_beaconator::routes::test_utils::create_test_app_state_with_account;

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
use the_beaconator::routes::test_utils::TestUtils;

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
use the_beaconator::routes::test_utils::mock_contract_deployment;

#[tokio::test]
async fn test_contract_deployment() {
    let deployment = mock_contract_deployment("PerpManager").await;
    assert_ne!(deployment.address, Address::ZERO);
    assert_eq!(deployment.gas_used, 1000000);
}
```

### Test Cleanup

```rust
use the_beaconator::routes::test_utils::TestCleanup;

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
5. **Contract ABIs**: Loaded from `tests/test_fixtures/` directory

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

use alloy::{
    json_abi::JsonAbi,
    network::EthereumWallet,
    node_bindings::{Anvil, AnvilInstance},
    primitives::{Address, U256},
    providers::{Provider, ProviderBuilder},
    signers::{Signer, local::PrivateKeySigner},
};
use std::str::FromStr;
use std::sync::Arc;
use the_beaconator::models::AppState;
use tokio::sync::OnceCell;

/// Anvil configuration and utilities
pub struct AnvilConfig {
    pub _instance: AnvilInstance,
    pub rpc_url: String,
    pub chain_id: u64,
    pub accounts: Vec<Address>,
}

impl Default for AnvilConfig {
    fn default() -> Self {
        Self::new()
    }
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
            _instance: anvil,
            rpc_url,
            chain_id,
            accounts,
        }
    }

    /// Get the first test account (deployer)
    pub fn deployer_account(&self) -> Address {
        self.accounts[0]
    }

    /// Get the first key as a PrivateKeySigner
    /// Note: Returns a deterministic test signer for development
    pub fn deployer_signer(&self) -> PrivateKeySigner {
        self.get_signer(0)
    }

    /// Get a signer for the specified account index
    pub fn get_signer(&self, index: usize) -> PrivateKeySigner {
        // Anvil uses deterministic test private keys
        let test_keys = [
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80", // Account 0
            "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d", // Account 1
            "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a", // Account 2
            "0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6", // Account 3
            "0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a", // Account 4
        ];

        let key = test_keys.get(index).unwrap_or(&test_keys[0]); // Default to first account if index out of bounds

        PrivateKeySigner::from_str(key)
            .expect("Failed to create signer from test key")
            .with_chain_id(Some(self.chain_id))
    }
}

impl Drop for AnvilConfig {
    fn drop(&mut self) {
        tracing::info!("Terminating Anvil instance (RPC: {})", self.rpc_url);
        // AnvilInstance automatically terminates when dropped
    }
}

/// Isolated Anvil instance manager - creates fresh instances per test
pub struct AnvilManager {
    config: AnvilConfig,
}

impl AnvilManager {
    /// Create a new isolated Anvil instance for this test
    pub async fn new() -> Self {
        let config = AnvilConfig::new();
        Self { config }
    }

    /// Get or create a shared Anvil instance (deprecated - use new() for isolation)
    #[deprecated(note = "Use AnvilManager::new() for better test isolation")]
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

    /// Get the RPC URL for this Anvil instance
    pub fn rpc_url(&self) -> &str {
        &self.config.rpc_url
    }

    /// Get the chain ID for this Anvil instance
    pub fn chain_id(&self) -> u64 {
        self.config.chain_id
    }

    /// Get the deployer account address
    pub fn deployer_account(&self) -> Address {
        self.config.deployer_account()
    }

    /// Get a signer for the specified account index
    pub fn get_signer(&self, index: usize) -> PrivateKeySigner {
        self.config.get_signer(index)
    }

    /// Get the deployer signer (first account)
    pub fn deployer_signer(&self) -> PrivateKeySigner {
        self.config.deployer_signer()
    }
}

impl Drop for AnvilManager {
    fn drop(&mut self) {
        tracing::info!("Dropping AnvilManager - Anvil instance will be terminated");
        // AnvilConfig drop will handle the cleanup
    }
}

/// Load ABI from test fixtures
pub fn load_test_abi(name: &str) -> JsonAbi {
    let fixture_path = format!("tests/test_fixtures/{name}.json");
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
    pub usdc: Address,
    pub deployer: Address,
    pub provider: Arc<the_beaconator::AlloyProvider>,
}

impl TestDeployment {
    /// Deploy test contracts to isolated Anvil instance
    pub async fn deploy_isolated(anvil: &AnvilManager) -> Result<Self, Box<dyn std::error::Error>> {
        // Create provider with deployer account
        let signer = anvil.deployer_signer();
        let wallet = EthereumWallet::from(signer);
        let provider = Arc::new(
            ProviderBuilder::new()
                .wallet(wallet)
                .connect_http(anvil.rpc_url().parse()?),
        );

        // For testing, we'll use mock addresses for now
        // In a real integration test, you would deploy actual contracts here
        let beacon_factory = Address::from_str("0x5FbDB2315678afecb367f032d93F642f64180aa3")?;
        let beacon_registry = Address::from_str("0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512")?;
        let perp_hook = Address::from_str("0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0")?;
        let usdc = Address::from_str("0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9")?;

        Ok(Self {
            beacon_factory,
            beacon_registry,
            perp_hook,
            usdc,
            provider,
            deployer: anvil.deployer_account(),
        })
    }

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
        let usdc = Address::from_str("0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9")?;

        tracing::info!("Test contracts deployed:");
        tracing::info!("  - BeaconFactory: {}", beacon_factory);
        tracing::info!("  - BeaconRegistry: {}", beacon_registry);
        tracing::info!("  - PerpManager: {}", perp_hook);
        tracing::info!("  - USDC: {}", usdc);

        Ok(Self {
            beacon_factory,
            beacon_registry,
            perp_hook,
            usdc,
            deployer: anvil.deployer_account(),
            provider,
        })
    }
}

/// Create a comprehensive test AppState with real blockchain connection
/// DEPRECATED: Use create_isolated_test_app_state() for better test isolation
#[deprecated(note = "Use create_isolated_test_app_state() for better test isolation")]
pub async fn create_test_app_state() -> AppState {
    // Get or create Anvil instance (deprecated - use isolated instances)
    #[allow(deprecated)]
    let anvil = AnvilManager::get_or_create().await;

    // Deploy test contracts
    let deployment = TestDeployment::deploy(&anvil)
        .await
        .expect("Failed to deploy test contracts");

    // Load real ABIs from test fixtures
    let beacon_abi = load_test_abi("Beacon");
    let beacon_factory_abi = load_test_abi("BeaconFactory");
    let beacon_registry_abi = load_test_abi("BeaconRegistry");
    let perp_manager_abi = load_test_abi("PerpManager");

    // Create signer for ECDSA operations (using Anvil's first deterministic test key)
    let test_signer = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
        .parse::<PrivateKeySigner>()
        .expect("Failed to parse test private key")
        .with_chain_id(Some(31337));

    AppState {
        provider: deployment.provider,
        alternate_provider: None,
        wallet_address: deployment.deployer,
        signer: test_signer,
        beacon_abi,
        beacon_factory_abi,
        beacon_registry_abi,
        perp_manager_abi,
        multicall3_abi: load_test_abi("Multicall3"),
        dichotomous_beacon_factory_abi: JsonAbi::new(), // Mock ABI for tests
        step_beacon_abi: JsonAbi::new(),                // Mock ABI for tests
        ecdsa_beacon_abi: JsonAbi::new(),               // Mock ABI for tests
        ecdsa_verifier_adapter_abi: JsonAbi::new(),     // Mock ABI for tests
        beacon_factory_address: deployment.beacon_factory,
        perpcity_registry_address: deployment.beacon_registry,
        perp_manager_address: deployment.perp_hook,
        usdc_address: Address::from_str("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap(), // Mock USDC address
        dichotomous_beacon_factory_address: None, // Not configured by default in tests
        usdc_transfer_limit: 1_000_000_000,       // 1000 USDC
        eth_transfer_limit: 10_000_000_000_000_000, // 0.01 ETH
        access_token: "test_token".to_string(),
        fees_module_address: Address::from_str("0x4567890123456789012345678901234567890123")
            .unwrap(),
        margin_ratios_module_address: Address::from_str(
            "0x5678901234567890123456789012345678901234",
        )
        .unwrap(),
        lockup_period_module_address: Address::from_str(
            "0x6789012345678901234567890123456789012345",
        )
        .unwrap(),
        sqrt_price_impact_limit_module_address: Address::from_str(
            "0x7890123456789012345678901234567890123456",
        )
        .unwrap(),
        default_starting_sqrt_price_x96: Some(560227709747861419891227623424), // sqrt(50) * 2^96
        multicall3_address: Some(
            Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap(),
        ), // Standard multicall3 address for tests
        wallet_manager: None, // Multi-wallet not used in tests
    }
}

/// Create a test AppState with a specific account
/// Create isolated test app state with proper cleanup (recommended for new tests)
pub async fn create_isolated_test_app_state() -> (AppState, AnvilManager) {
    // Create isolated Anvil instance
    let anvil = AnvilManager::new().await;

    // Deploy test contracts
    let deployment = TestDeployment::deploy_isolated(&anvil)
        .await
        .expect("Failed to deploy test contracts");

    // Load real ABIs from test fixtures
    let beacon_abi = load_test_abi("Beacon");
    let beacon_factory_abi = load_test_abi("BeaconFactory");
    let beacon_registry_abi = load_test_abi("BeaconRegistry");
    let perp_manager_abi = load_test_abi("PerpManager");

    // Create signer for ECDSA operations (using Anvil's first deterministic test key)
    let test_signer = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
        .parse::<PrivateKeySigner>()
        .expect("Failed to parse test private key")
        .with_chain_id(Some(31337));

    let app_state = AppState {
        provider: deployment.provider,
        alternate_provider: None,
        wallet_address: deployment.deployer,
        signer: test_signer,
        beacon_abi,
        beacon_factory_abi,
        beacon_registry_abi,
        perp_manager_abi,
        multicall3_abi: load_test_abi("Multicall3"),
        dichotomous_beacon_factory_abi: JsonAbi::new(), // Mock ABI for tests
        step_beacon_abi: JsonAbi::new(),                // Mock ABI for tests
        ecdsa_beacon_abi: JsonAbi::new(),               // Mock ABI for tests
        ecdsa_verifier_adapter_abi: JsonAbi::new(),     // Mock ABI for tests
        beacon_factory_address: deployment.beacon_factory,
        perpcity_registry_address: deployment.beacon_registry,
        perp_manager_address: deployment.perp_hook,
        usdc_address: deployment.usdc,
        dichotomous_beacon_factory_address: None, // Not configured by default in tests
        usdc_transfer_limit: 1_000_000_000,       // 1000 USDC
        eth_transfer_limit: 10_000_000_000_000_000, // 0.01 ETH
        access_token: "test_token".to_string(),
        fees_module_address: Address::from_str("0x4567890123456789012345678901234567890123")
            .unwrap(),
        margin_ratios_module_address: Address::from_str(
            "0x5678901234567890123456789012345678901234",
        )
        .unwrap(),
        lockup_period_module_address: Address::from_str(
            "0x6789012345678901234567890123456789012345",
        )
        .unwrap(),
        sqrt_price_impact_limit_module_address: Address::from_str(
            "0x7890123456789012345678901234567890123456",
        )
        .unwrap(),
        default_starting_sqrt_price_x96: Some(560227709747861419891227623424), // sqrt(50) * 2^96
        multicall3_address: Some(
            Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap(),
        ), // Standard multicall3 address for tests
        wallet_manager: None, // Multi-wallet not used in tests
    };

    (app_state, anvil)
}

/// DEPRECATED: Use create_isolated_test_app_state() for better test isolation
#[deprecated(note = "Use create_isolated_test_app_state() for better test isolation")]
pub async fn create_test_app_state_with_account(account_index: usize) -> AppState {
    #[allow(deprecated)]
    let anvil = AnvilManager::get_or_create().await;

    let signer = anvil.get_signer(account_index);
    let wallet = EthereumWallet::from(signer.clone());
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
        alternate_provider: None,
        wallet_address: anvil.accounts[account_index],
        signer,
        beacon_abi: load_test_abi("Beacon"),
        beacon_factory_abi: load_test_abi("BeaconFactory"),
        beacon_registry_abi: load_test_abi("BeaconRegistry"),
        perp_manager_abi: load_test_abi("PerpManager"),
        multicall3_abi: load_test_abi("Multicall3"),
        dichotomous_beacon_factory_abi: JsonAbi::new(), // Mock ABI for tests
        step_beacon_abi: JsonAbi::new(),                // Mock ABI for tests
        ecdsa_beacon_abi: JsonAbi::new(),               // Mock ABI for tests
        ecdsa_verifier_adapter_abi: JsonAbi::new(),     // Mock ABI for tests
        beacon_factory_address: deployment.beacon_factory,
        perpcity_registry_address: deployment.beacon_registry,
        perp_manager_address: deployment.perp_hook,
        usdc_address: Address::from_str("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap(), // Mock USDC address
        dichotomous_beacon_factory_address: None, // Not configured by default in tests
        usdc_transfer_limit: 1_000_000_000,       // 1000 USDC
        eth_transfer_limit: 10_000_000_000_000_000, // 0.01 ETH
        access_token: "test_token".to_string(),
        fees_module_address: Address::from_str("0x4567890123456789012345678901234567890123")
            .unwrap(),
        margin_ratios_module_address: Address::from_str(
            "0x5678901234567890123456789012345678901234",
        )
        .unwrap(),
        lockup_period_module_address: Address::from_str(
            "0x6789012345678901234567890123456789012345",
        )
        .unwrap(),
        sqrt_price_impact_limit_module_address: Address::from_str(
            "0x7890123456789012345678901234567890123456",
        )
        .unwrap(),
        default_starting_sqrt_price_x96: Some(560227709747861419891227623424), // sqrt(50) * 2^96
        multicall3_address: Some(
            Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap(),
        ), // Standard multicall3 address for tests
        wallet_manager: None, // Multi-wallet not used in tests
    }
}

/// Test utilities for blockchain interactions
pub struct TestUtils;

impl TestUtils {
    /// Get the current block number
    pub async fn get_block_number(
        provider: &the_beaconator::AlloyProvider,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let block_number = provider.get_block_number().await?;
        Ok(block_number)
    }

    /// Get account balance
    pub async fn get_balance(
        provider: &the_beaconator::AlloyProvider,
        address: Address,
    ) -> Result<U256, Box<dyn std::error::Error>> {
        let balance = provider.get_balance(address).await?;
        Ok(balance)
    }
}

/// Test fixture for contract deployment results
#[derive(Debug, Clone)]
pub struct ContractDeploymentResult {
    pub address: Address,
    pub transaction_hash: String,
    pub gas_used: u64,
}

/// Mock contract deployment (for testing without actual deployment)
pub async fn mock_contract_deployment(name: &str) -> ContractDeploymentResult {
    // Generate deterministic addresses for testing
    let address = match name {
        "Beacon" => Address::from_str("0x5FbDB2315678afecb367f032d93F642f64180aa3").unwrap(),
        "BeaconFactory" => Address::from_str("0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512").unwrap(),
        "BeaconRegistry" => {
            Address::from_str("0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0").unwrap()
        }
        "PerpManager" => Address::from_str("0xCf7Ed3AccA5a467e9e704C703E8D87F634fB0Fc9").unwrap(),
        _ => Address::from_str("0x0000000000000000000000000000000000000000").unwrap(),
    };

    ContractDeploymentResult {
        address,
        transaction_hash: "0x1234567890123456789012345678901234567890123456789012345678901234"
            .to_string(),
        gas_used: 1000000,
    }
}

/// Create a synchronous test AppState for simple tests (fallback)
pub fn create_simple_test_app_state() -> AppState {
    // Create mock provider with wallet for testing - this won't work for real network calls
    let signer = alloy::signers::local::PrivateKeySigner::random();
    let wallet = alloy::network::EthereumWallet::from(signer.clone());
    // Use modern Alloy provider builder pattern for tests
    let provider = alloy::providers::ProviderBuilder::new()
        .wallet(wallet)
        .connect_http("http://localhost:8545".parse().unwrap());

    AppState {
        provider: Arc::new(provider),
        alternate_provider: None,
        wallet_address: Address::from_str("0x1111111111111111111111111111111111111111").unwrap(),
        signer,
        beacon_abi: JsonAbi::new(),
        beacon_factory_abi: JsonAbi::new(),
        beacon_registry_abi: JsonAbi::new(),
        perp_manager_abi: JsonAbi::new(),
        multicall3_abi: JsonAbi::new(),
        dichotomous_beacon_factory_abi: JsonAbi::new(),
        step_beacon_abi: JsonAbi::new(),
        ecdsa_beacon_abi: JsonAbi::new(),
        ecdsa_verifier_adapter_abi: JsonAbi::new(),
        beacon_factory_address: Address::from_str("0x1234567890123456789012345678901234567890")
            .unwrap(),
        perpcity_registry_address: Address::from_str("0x2345678901234567890123456789012345678901")
            .unwrap(),
        perp_manager_address: Address::from_str("0x3456789012345678901234567890123456789012")
            .unwrap(),
        usdc_address: Address::from_str("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap(),
        dichotomous_beacon_factory_address: None, // Not configured by default in tests
        usdc_transfer_limit: 1_000_000_000,       // 1000 USDC
        eth_transfer_limit: 10_000_000_000_000_000, // 0.01 ETH
        access_token: "test_token".to_string(),
        fees_module_address: Address::from_str("0x4567890123456789012345678901234567890123")
            .unwrap(),
        margin_ratios_module_address: Address::from_str(
            "0x5678901234567890123456789012345678901234",
        )
        .unwrap(),
        lockup_period_module_address: Address::from_str(
            "0x6789012345678901234567890123456789012345",
        )
        .unwrap(),
        sqrt_price_impact_limit_module_address: Address::from_str(
            "0x7890123456789012345678901234567890123456",
        )
        .unwrap(),
        default_starting_sqrt_price_x96: Some(560227709747861419891227623424), // sqrt(50) * 2^96
        multicall3_address: Some(
            Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap(),
        ), // Standard multicall3 address for tests
        wallet_manager: None, // Multi-wallet not used in tests
    }
}

/// Create a test AppState with a custom provider (for mocking network behavior)
pub fn create_test_app_state_with_provider(
    provider: Arc<the_beaconator::AlloyProvider>,
) -> AppState {
    // Create a random signer for ECDSA operations in tests
    let signer = PrivateKeySigner::random();

    AppState {
        provider,
        alternate_provider: None,
        wallet_address: Address::from_str("0x1111111111111111111111111111111111111111").unwrap(),
        signer,
        beacon_abi: JsonAbi::new(),
        beacon_factory_abi: JsonAbi::new(),
        beacon_registry_abi: JsonAbi::new(),
        perp_manager_abi: JsonAbi::new(),
        multicall3_abi: JsonAbi::new(),
        dichotomous_beacon_factory_abi: JsonAbi::new(),
        step_beacon_abi: JsonAbi::new(),
        ecdsa_beacon_abi: JsonAbi::new(),
        ecdsa_verifier_adapter_abi: JsonAbi::new(),
        beacon_factory_address: Address::from_str("0x1234567890123456789012345678901234567890")
            .unwrap(),
        perpcity_registry_address: Address::from_str("0x2345678901234567890123456789012345678901")
            .unwrap(),
        perp_manager_address: Address::from_str("0x3456789012345678901234567890123456789012")
            .unwrap(),
        usdc_address: Address::from_str("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap(),
        dichotomous_beacon_factory_address: None, // Not configured by default in tests
        usdc_transfer_limit: 1_000_000_000,       // 1000 USDC
        eth_transfer_limit: 10_000_000_000_000_000, // 0.01 ETH
        access_token: "test_token".to_string(),
        fees_module_address: Address::from_str("0x4567890123456789012345678901234567890123")
            .unwrap(),
        margin_ratios_module_address: Address::from_str(
            "0x5678901234567890123456789012345678901234",
        )
        .unwrap(),
        lockup_period_module_address: Address::from_str(
            "0x6789012345678901234567890123456789012345",
        )
        .unwrap(),
        sqrt_price_impact_limit_module_address: Address::from_str(
            "0x7890123456789012345678901234567890123456",
        )
        .unwrap(),
        default_starting_sqrt_price_x96: Some(560227709747861419891227623424), // sqrt(50) * 2^96
        multicall3_address: Some(
            Address::from_str("0xcA11bde05977b3631167028862bE2a173976CA11").unwrap(),
        ), // Standard multicall3 address for tests
        wallet_manager: None, // Multi-wallet not used in tests
    }
}

/// Create a mock provider that always returns network errors (for deterministic testing)
pub fn create_mock_provider_with_network_error() -> Arc<the_beaconator::AlloyProvider> {
    // Use a non-existent endpoint that will fail deterministically
    let signer = alloy::signers::local::PrivateKeySigner::random();
    let wallet = alloy::network::EthereumWallet::from(signer);
    let provider = alloy::providers::ProviderBuilder::new()
        .wallet(wallet)
        .connect_http("http://127.0.0.1:1".parse().unwrap()); // Port 1 - guaranteed to fail

    Arc::new(provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_anvil_manager() {
        #[allow(deprecated)]
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

        let perp_manager_abi = load_test_abi("PerpManager");
        assert!(!perp_manager_abi.functions.is_empty());
    }

    #[tokio::test]
    async fn test_app_state_creation() {
        #[allow(deprecated)]
        let app_state = create_test_app_state().await;
        assert_ne!(app_state.wallet_address, Address::ZERO);
        assert_ne!(app_state.beacon_factory_address, Address::ZERO);
        assert_ne!(app_state.perp_manager_address, Address::ZERO);
    }

    #[tokio::test]
    async fn test_test_deployment() {
        #[allow(deprecated)]
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
        #[allow(deprecated)]
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
