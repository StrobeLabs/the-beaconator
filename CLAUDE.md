# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# The Beaconator

**The Beaconator** is a Rust web service that manages Ethereum beacon contracts and perpetual deployments on Arbitrum. It provides RESTful APIs for creating beacons, deploying perpetuals, and updating beacon data with zero-knowledge proofs.

## Pinned contract versions

The-beaconator's REST surface and inline `sol!` interfaces target specific contract release tags (not main):

- `perpcity-contracts` @ **v0.1.0** (`bffbd6f`, 2026-04 — "isolated per-perp markets" architecture: `PerpFactory` + per-market `Perp` contracts)
- `beacons` @ **v0.0.1** (`d61aca0`, 2026-04-22)

The pin is recorded in `.contracts-versions`. After bumping it, regenerate the JSON ABIs via:

```bash
make refresh-abis
```

The script `scripts/refresh-abis.sh` adds a temporary git worktree at each pinned tag, initializes submodules (rewriting SSH→HTTPS), runs `forge inspect <Contract> abi --json`, and writes back into `abis/`.

## Tech Stack
- **Framework**: Rocket 0.5.0 (async web framework)
- **Blockchain**: Alloy 1.5 (Ethereum library)
- **Network**: Arbitrum (mainnet = Arbitrum One, testnet = Arbitrum Sepolia)
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
- **`src/models/`**: Request/response models and AppState definition
  - `component_factory.rs`: 20 component factory types and configs
  - `recipe.rs`: Beacon recipes and spec enums (BeaconKind, PreprocessorSpec, etc.)
- **`src/routes/`**: API endpoint implementations
  - `beacon.rs`: Beacon operations (create, update, batch)
  - `perp.rs`: Perpetual operations
  - `recipe.rs`: Recipe and component factory listing endpoints
  - `wallet.rs`: Wallet funding
  - `mod.rs`: Shared utilities, sol! interfaces, and transaction management
- **`src/services/beacon/`**: Beacon service layer
  - `modular.rs`: Multi-step modular beacon creation orchestrator
  - `component_registry.rs`: Redis-backed component factory address registry
  - `recipe_registry.rs`: Redis-backed recipe registry with 12 standard recipes
- **`src/guards.rs`**: Authentication guard for Bearer token validation
- **`src/main.rs`**: Entry point that launches Rocket server
- **`abis/`**: JSON ABI snapshots regenerated from pinned contract tags via `make refresh-abis`. NOT loaded at runtime — see `src/routes/mod.rs` for the inline `sol!` interfaces the service actually binds against. JSONs ship as a reference for client SDK generators and human inspection.

### Code Organization
See `ARCHITECTURE.md` for detailed guidelines on code organization and best practices for managing large files.

### Key API Endpoints

**Production-Ready:**
- `POST /create_perpcity_beacon` - Create single beacon
- `POST /batch_create_perpcity_beacon` - Batch create beacons (1-100 limit)
- `POST /deploy_perp_for_beacon` - Deploy perp for single beacon
- `POST /update_beacon` - Update beacon with ZK proof
- `POST /create_modular_beacon` - Create beacon using modular recipe system
- `GET /recipes` - List all available beacon recipes
- `GET /recipes/<slug>` - Get specific recipe details
- `GET /component_factories` - List all component factory addresses

### Alloy Provider Architecture

**Critical:** Alloy uses complex nested generic types for providers. The project uses a concrete type definition (`AlloyProvider`) instead of trait objects because the `sol!` macro requires concrete types.

Key patterns:
- Provider setup in `src/lib.rs:50-90` using `ProviderBuilder::new().wallet(wallet).connect_http(url)`
- Contract instantiation uses `&*state.provider` to dereference Arc
- AppState stores wallet address separately for easy access

## API Documentation

### OpenAPI Integration

The API is fully documented with OpenAPI 3.0 using `rocket_okapi`:
- **OpenAPI spec**: Served at `/openapi.json` when server is running
- **Spec generation**: Start the server and download from `/openapi.json`

All endpoints are annotated with `#[openapi(tag = "...")]` macros for automatic documentation generation.

### Generating API Clients

The OpenAPI spec can be used to generate type-safe clients in any language:

**TypeScript:**
```bash
npm install -D openapi-typescript
npx openapi-typescript openapi.json -o api.ts
```

**Python:**
```bash
pipx install openapi-python-client
openapi-python-client generate --path openapi.json --output-path client/python
```

**Other languages:** Use any OpenAPI 3.0 code generator (e.g., openapi-generator, Swagger Codegen).

### Viewing API Documentation

Use any OpenAPI viewer to explore the API interactively:
- RapiDoc: `npx serve` then open with RapiDoc
- Swagger UI: Upload `openapi.json`
- Redoc: `npx @redocly/cli preview-docs openapi.json`

## Environment Configuration

Copy `env.example` to `.env.local` (preferred) or `.env` and configure. Required env vars for the v0.1.0 contract pin:

```bash
RPC_URL=https://arb1.arbitrum.io/rpc          # Arbitrum RPC URL
ENV=mainnet|testnet|localnet                  # mainnet=Arbitrum One (42161), testnet=Arbitrum Sepolia (421614)
BEACONATOR_ACCESS_TOKEN=your_secret_token     # API authentication
PRIVATE_KEY=...                               # Funding wallet private key (no 0x prefix)
WALLET_PRIVATE_KEYS=...                       # Comma-separated wallet pool keys
PERPCITY_REGISTRY_ADDRESS=0x...               # BeaconRegistry (beacons@v0.0.1)
PERP_FACTORY_ADDRESS=0x...                    # PerpFactory (perpcity-contracts@v0.1.0)
ECDSA_VERIFIER_FACTORY_ADDRESS=0x...          # ECDSAVerifierFactory (beacons@v0.0.1)
USDC_ADDRESS=0x...                            # USDC ERC20 (network-specific)

# Modules struct for PerpFactory.createPerp (perpcity-contracts@v0.1.0)
FEES_MODULE_ADDRESS=0x...
FUNDING_MODULE_ADDRESS=0x...                  # NEW in v0.1.0 (replaces lockup-period semantics)
MARGIN_RATIOS_MODULE_ADDRESS=0x...
PRICE_IMPACT_MODULE_ADDRESS=0x...             # NEW in v0.1.0 (replaces sqrt-price-impact-limit)
PRICING_MODULE_ADDRESS=0x...                  # NEW in v0.1.0

# Optional / governance — not needed for the create / open flow
PROTOCOL_FEE_MANAGER_ADDRESS=0x...
MODULE_REGISTRY_ADDRESS=0x...
MULTICALL3_ADDRESS=0xcA11bde05977b3631167028862bE2a173976CA11
```

## Implementation Details

### AppState Structure (`src/models/app_state.rs`)
- Arc'd Alloy provider with wallet integration
- Separate wallet address storage for easy access
- Contract addresses loaded from env vars at startup (`src/lib.rs:96-165`)
- All contract addresses and access token

### Contract Interactions (`src/routes/mod.rs`)
- Inline `sol!` macros define `IBeacon`, `IBeaconRegistry`, `IPerpFactory`, `IPerp`, factories, etc.
- v0.1.0 architecture: `PerpFactory.createPerp()` returns a per-market `Perp` address; subsequent `openMaker` / `openTaker` calls go to that address.
- Dereferences Arc provider with `&*state.provider` for read calls; uses wallet-bound provider from the pool for writes.
- Comprehensive error handling with Sentry integration; revert reasons decoded via `services::perp::validation::ContractErrorDecoder`.

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
// CORRECT - Current Alloy v1.5+ pattern
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
- Inline `sol!` macros in `src/routes/mod.rs` are the source of truth for what the service binds against. Update those when the pinned contracts change.
- JSON files in `abis/` are reference snapshots regenerated from `forge inspect` against the pinned tags via `make refresh-abis`. They are NOT loaded by the runtime — they exist for OpenAPI client generators and for human inspection.
- **Known gap (forge limitation):** `abis/Perp.json` is missing the `MakerOpened`, `TakerOpened`, `Maker*` / `Taker*Adjusted` / `*Closed` / `*Backstopped` and Tick/funding/cumulatives events. Those are declared as free events in `perpcity-contracts/src/libraries/Events.sol` and emitted from the `PerpLogic` library, but `forge inspect Perp abi` doesn't propagate library-declared free events into a contract's ABI. The Rust runtime decodes them anyway via the inline `IPerp { event MakerOpened(...); ... }` block, so service code is unaffected. Downstream SDK generators that need event signatures should consult either the inline `sol!` block or `Events.sol` directly.
- The pinned tags are recorded in `.contracts-versions`. CI validates that `git diff abis/` is clean after a refresh, so a stale `abis/` will fail CI on the next refresh.

## Modular Beacon Creation

The beacon system uses a modular architecture with 20 individual component factory contracts. Beacons are created by composing components from separate factories.

### Component Factory Types
- **Preprocessors**: IdentityPreprocessor, Threshold, TernaryToBinary, Argmax
- **Base Functions**: CGBM (continuous), DGBM (discrete)
- **Transforms**: Bounded (sigmoid), Unbounded (exponential)
- **Composers**: WeightedSum
- **Group Functions**: Dominance, RelativeDominance, ContinuousAllocation, DiscreteAllocation
- **Group Transforms**: Softmax, GMNormalize
- **Beacon Factories**: Identity, Standalone, Composite, GroupManager
- **Verifier**: ECDSAVerifier

### Recipe System
Standard recipes define which component factories to call. 12 built-in recipes are seeded at startup:
- 8 Standalone: lbcgbm, cgbm, lbdgbm, dgbm, ternary_lbcgbm, ternary_cgbm, ternary_lbdgbm, ternary_dgbm
- 4 Group: dominance, relative_dominance, discrete_allocation, continuous_allocation

### Redis Storage
Factory addresses are pre-seeded into Redis (not read from env vars at request time). Keys:
- `beaconator:component_factory:{FactoryType}` - factory config JSON
- `beaconator:component_factories` - set of factory type names
- `beaconator:beacon_recipe:{slug}` - recipe config JSON
- `beaconator:beacon_recipes` - set of recipe slugs

Seeding paths: direct Redis writes, or the optional `COMPONENT_FACTORIES_JSON`
env var — a `{"FactoryType": "0xaddr"}` map seeded at startup without
overwriting existing entries (use this when the Redis instance cannot be seeded
by hand).

### Creation Flow
1. Look up recipe by slug from RecipeRegistry
2. Acquire wallet from pool (held for all steps)
3. Create ECDSA verifier via ECDSAVerifierFactory
4. Create components via appropriate factories (preprocessor, baseFn, transform, etc.)
5. Create beacon via beacon factory (Standalone/Composite/GroupManager), passing component addresses
6. Register with beacon registry

## Perp Deployment — v0.1.0 architecture

`perpcity-contracts@v0.1.0` introduces an **isolated per-perp markets** model (commit `ee6e396`). The old single `PerpManager` is gone — each market is its own `Perp` contract created by a shared `PerpFactory`.

### Topology
- **`PerpFactory`** (one per network): deploys per-market `Perp` contracts, initializes a Uniswap V4 pool, and emits `PerpCreated` with the new `Perp` address.
- **`Perp`** (one per market): ERC721 of position NFTs, holds the V4 pool's accounting tokens, exposes `openMaker` / `openTaker` / `adjust*` / `liquidate*` / `backstop*` plus view functions for fees / open interest / capacity / rates / EMAs / cumulatives.
- **`ProtocolFeeManager`** (one per network): owner-controlled, returns `protocolFee()` consumed by `Perp`.
- **`ModuleRegistry`** (one per network): governance allowlist of acceptable module implementations. Not on the deploy fast path.

### Module set (`Modules` struct passed to `PerpFactory.createPerp`)
Five module addresses, all required, all deployed once and reused across markets:
- **`fees`** (`IFees`): trading fees (creator / insurance / LP), liquidation fee, utilization fees.
- **`funding`** (`IFunding`, NEW in v0.1.0): per-second funding rate as a function of spots and EMAs.
- **`marginRatios`** (`IMarginRatios`): initial / liquidation / backstop margin ratios for maker and taker.
- **`priceImpact`** (`IPriceImpact`, NEW in v0.1.0): dynamic sqrt-price bounds for swaps.
- **`pricing`** (`IPricing`, NEW in v0.1.0): `markPrice` from amm/index/EMA inputs.

### Deploy → open flow (the-beaconator)
1. `POST /create_perpcity_beacon` — deploy a beacon and register it with `BeaconRegistry`.
2. `POST /deploy_perp_for_beacon` with `{ beacon_address, owner, name, symbol, token_uri, ema_window, salt? }`.
   - Server reads module addresses from env, builds the `Modules` struct, calls `PerpFactory.createPerp`.
   - Parses `PerpCreated` event from the receipt to extract the new per-market `Perp` address.
   - Returns `{ perp_address, pool_id, perp_factory_address, sqrt_price_x96, tick, initial_index, ema_window, transaction_hash }`.
3. `POST /deposit_liquidity_for_perp` with `{ perp_address, margin_amount_usdc, tick_*? }`.
   - Server approves USDC against the per-market `Perp` contract (per-perp pulls via `safeTransferFrom`), then calls `Perp.openMaker(OpenMakerParams)`.
   - Parses `MakerOpened` event for the position id.

### Implementation notes
- Approve target for USDC is the **per-Perp address**, NOT the factory.
- `OpenMakerParams.liquidity` was widened from `uint120` (v0.0.1) to `uint128` (v0.1.0); `maxAmt0In` / `maxAmt1In` from `uint128` to `uint256`. The Rust DTOs reflect this.
- `MakerOpened` and `TakerOpened` are now separate events (the old `PositionOpened` unified event no longer exists).

## Error Handling Standards
- All API endpoints return standardized `ApiResponse<T>` format
- Sentry integration for production error tracking
- Batch operations continue on individual failures with detailed error reporting
- Network errors gracefully handled in test environment

## Git Commit Guidelines
- Use concise commit messages (1 sentence max)
- Do not include Claude Code attribution or co-author tags (private repo)
- Focus on the specific change made