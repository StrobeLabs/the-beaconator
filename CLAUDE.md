# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# The Beaconator

**The Beaconator** is a Rust web service that manages Ethereum beacon contracts and perpetual deployments on Base network. It provides RESTful APIs for creating beacons, deploying perpetuals, and updating beacon data with zero-knowledge proofs.

## Tech Stack
- **Framework**: Rocket 0.5.0 (async web framework)
- **Blockchain**: Alloy 1.0.17 (Ethereum library)
- **Network**: Base mainnet/testnet support
- **Security**: Bearer token authentication
- **Monitoring**: Sentry integration + structured logging

## Development Commands

Use the Makefile for all development tasks:

```bash
make dev          # Start development server
make test-fast    # Run quick unit tests (recommended for development)
make test         # Run full test suite (unit + integration)
make quality      # Full quality check (format + lint + fast tests)
make lint         # Run clippy linter with strict warnings
make fmt          # Format code with rustfmt
make build        # Build project (debug)
make build-release # Build project (release)
```

### Test Strategy
- `make test-fast`: Quick unit tests only (~1s) - use during development
- `make test-integration`: Integration tests with Anvil (~15s)
- `make test-full`: Complete test suite - use before commits
- `make quality`: Pre-commit checks with fast tests only

## Architecture Overview

### Core Components

- **`src/lib.rs`**: Provider setup, ABI loading, and app initialization
- **`src/models.rs`**: Request/response models and AppState definition
- **`src/routes/`**: API endpoint implementations
  - `beacon.rs` (2,828 lines): Beacon operations
  - `perp.rs` (3,351 lines): Perpetual operations
  - `wallet.rs`: Wallet funding
  - `mod.rs`: Shared utilities and transaction management
- **`src/guards.rs`**: Authentication guard for Bearer token validation
- **`src/main.rs`**: Entry point that launches Rocket server
- **`abis/`**: Contract ABI files loaded at runtime

### Code Organization
See `ARCHITECTURE.md` for detailed guidelines on code organization and best practices for managing large files.

### Key API Endpoints

**Production-Ready:**
- `POST /create_perpcity_beacon` - Create single beacon
- `POST /batch_create_perpcity_beacon` - Batch create beacons (1-100 limit)
- `POST /deploy_perp_for_beacon` - Deploy perp for single beacon
- `POST /update_beacon` - Update beacon with ZK proof

**Placeholder (not implemented):**
- `GET /all_beacons`
- `POST /create_beacon` 
- `POST /register_beacon`

### Alloy Provider Architecture

**Critical:** Alloy uses complex nested generic types for providers. The project uses a concrete type definition (`AlloyProvider`) instead of trait objects because the `sol!` macro requires concrete types.

Key patterns:
- Provider setup in `src/lib.rs:50-90` using `ProviderBuilder::new().wallet(wallet).connect_http(url)`
- Contract instantiation uses `&*state.provider` to dereference Arc
- AppState stores wallet address separately for easy access

## Environment Configuration

Copy `env.example` to `.env` and configure:

```bash
RPC_URL=https://mainnet.base.org           # Base chain RPC URL
ENV=mainnet|testnet|localnet               # Network type
BEACONATOR_ACCESS_TOKEN=your_secret_token  # API authentication  
PRIVATE_KEY=0x...                          # Wallet private key (without 0x)
BEACON_FACTORY_ADDRESS=0x...               # Factory contract address
PERPCITY_REGISTRY_ADDRESS=0x...            # Registry contract address  
PERP_HOOK_ADDRESS=0x...                    # PerpHook contract address
```

## Implementation Details

### AppState Structure (`src/models.rs:7-18`)
- Arc'd Alloy provider with wallet integration
- Separate wallet address storage for easy access
- Contract ABIs loaded from `abis/` directory at startup
- All contract addresses and access token

### Contract Interactions (`src/routes.rs`)
- Uses Alloy's `sol!` macro for type-safe contract interfaces
- Dereferences Arc provider with `&*state.provider` for contract calls
- Comprehensive error handling with Sentry integration
- Batch operations support partial failures

### Testing Strategy
- Mock providers with wallets for consistent test setup
- Uses `#[tokio::test]` for async tests
- Network calls fail gracefully in test environment
- Serial test execution with `#[serial]` for stateful tests

### Anvil Resource Optimization
- Tests use shared Anvil instances to reduce memory overhead
- Regular cleanup recommended after test runs using `scripts/anvil-cleanup.sh` or `clean-anvil` alias
- For manual Anvil usage, use resource-optimized settings:
  - `anvil --state-interval 0 --no-storage-caching --memory-limit 128`
- Test Anvil instances use 1-second block times for faster execution

## Alloy Best Practices

### Modern Provider Pattern
```rust
// CORRECT - Current Alloy v1.0+ pattern
let provider = ProviderBuilder::new()
    .wallet(wallet)
    .connect_http(url);

// AVOID - Deprecated pattern
let provider = ProviderBuilder::new()
    .wallet(wallet)  
    .on_http(url);     // Deprecated in v1.0+
```

### Contract Interface Pattern
```rust
// CORRECT - Use sol! macro for type safety
sol! {
    interface IBeacon {
        function updateData(bytes calldata proof, bytes calldata publicSignals) external;
    }
}

// Use with Arc provider dereference
let contract = IBeacon::new(address, &*state.provider);
```

### ABI Management
- ABIs stored in `abis/` directory and loaded at runtime via `load_abi()` 
- File-based approach preferred over embedded serde structs
- Manual ABI modifications documented (e.g., createPerp function added to PerpHook.json)

## Perp Deployment Configuration

The perp deployment endpoints use hardcoded defaults from `DeployPerp.s.sol`:
- Trading fee: 0.5% (50 basis points)
- Leverage range: 0-10x (min/max)
- Liquidation leverage: 10x
- Starting price: sqrt(50) * 2^96
- Tick spacing: 30
- Funding interval: 1 day (86400 seconds)

## Error Handling Standards
- All API endpoints return standardized `ApiResponse<T>` format
- Sentry integration for production error tracking
- Batch operations continue on individual failures with detailed error reporting
- Network errors gracefully handled in test environment

## Git Commit Guidelines
- Use concise commit messages (1 sentence max)
- Do not include Claude Code attribution or co-author tags (private repo)
- Focus on the specific change made