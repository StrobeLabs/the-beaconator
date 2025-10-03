# The Beaconator Architecture Guide

## Current Structure

The Beaconator follows a modular Rocket.rs web service pattern with the following structure:

```
src/
├── main.rs              # Entry point
├── lib.rs              # Core initialization and provider setup
├── guards.rs           # Authentication guards (Bearer token)
├── fairings.rs         # Request/response middleware
├── models/             # Data models module
│   ├── mod.rs          # Public exports
│   ├── app_state.rs    # AppState, API metadata, PerpConfig
│   ├── requests.rs     # Request DTOs
│   └── responses.rs    # Response DTOs and ApiResponse wrapper
├── routes/             # HTTP endpoint handlers
│   ├── mod.rs          # Route mounting and shared utilities
│   ├── beacon.rs       # Beacon route handlers (~396 lines)
│   ├── perp.rs         # Perpetual route handlers (~1,790 lines)
│   ├── wallet.rs       # Wallet funding handlers (~279 lines)
│   └── info.rs         # API documentation endpoints (~41 lines)
└── services/           # Business logic layer
    ├── beacon/         # Beacon service logic
    │   ├── mod.rs      # Public exports
    │   ├── core.rs     # Core beacon operations (~695 lines)
    │   ├── batch.rs    # Batch beacon operations (~451 lines)
    │   └── verifiable.rs # Verifiable beacon logic (~157 lines)
    ├── perp/           # Perpetual service logic
    │   ├── mod.rs      # Public exports
    │   └── operations.rs # Perp operations (~1,120 lines)
    └── transaction/    # Transaction management
        ├── mod.rs      # Public exports
        ├── execution.rs # Transaction execution (~100 lines)
        ├── events.rs   # Event parsing and verification (~197 lines)
        └── multicall.rs # Multicall3 utilities (~212 lines)
```

## Code Organization Guidelines

### For New Features

When adding new features, follow these patterns:

1. **Small Features (<500 lines)**:
   - Add to existing module if related (beacon.rs, perp.rs, wallet.rs)
   - Create new file in routes/ if distinct domain

2. **Large Features (>500 lines)**:
   - Create a subdirectory under routes/
   - Split into logical components:
     ```
     routes/your_feature/
     ├── mod.rs         # Public exports and route mounting
     ├── handlers.rs    # HTTP endpoint handlers
     ├── logic.rs       # Business logic
     └── tests.rs       # Unit tests
     ```

3. **Shared Logic**:
   - Transaction utilities → `routes/mod.rs`
   - Common models → `models/` module
   - Authentication → `guards.rs`
   - Middleware → `fairings.rs`

### Module Responsibilities

#### Routes Layer (`routes/`)
HTTP endpoint handlers that receive requests and return responses:
- **`beacon.rs`**: Beacon route handlers (create, batch create, update, batch update, verifiable beacons)
- **`perp.rs`**: Perpetual route handlers (deploy, batch deploy, liquidity management)
- **`wallet.rs`**: Wallet funding endpoints (guest wallet funding with ETH/USDC)
- **`info.rs`**: API documentation and status endpoints
- **`mod.rs`**: Shared route utilities and transaction helpers

See [README.md](README.md#api-endpoints) for complete API documentation.

#### Services Layer (`services/`)
Business logic separated from HTTP concerns:

**Beacon Services (`services/beacon/`)**:
- **`core.rs`**: Core beacon operations (creation via factory, registration)
- **`batch.rs`**: Batch beacon operations using Multicall3
- **`verifiable.rs`**: Verifiable beacon logic with ZK proof handling

**Perp Services (`services/perp/`)**:
- **`operations.rs`**: Perpetual deployment and liquidity management logic

**Transaction Services (`services/transaction/`)**:
- **`execution.rs`**: Transaction execution and serialization
- **`events.rs`**: Event parsing and verification from transaction receipts
- **`multicall.rs`**: Multicall3 utilities for batching contract calls

#### Models Layer (`models/`)
Data structures and type definitions:
- **`app_state.rs`**: Application state (AppState), API metadata, PerpConfig
- **`requests.rs`**: All request DTOs (beacon, perp, wallet operations)
- **`responses.rs`**: All response DTOs and ApiResponse wrapper

### Testing Strategy

Tests are organized as follows:

1. **Unit Tests** (`#[cfg(test)]` blocks):
   - Located in same file as code
   - Fast, no network calls
   - Run with: `make test-fast`

2. **Integration Tests** (in test modules):
   - Test with Anvil local blockchain
   - Single-threaded execution
   - Run with: `make test-integration`

3. **Full Test Suite**:
   - Run with: `make test-full`
   - CI uses: `make quality` (runs fast tests only)

### Best Practices

1. **File Size Management**:
   - Keep files under 1,500 lines when possible
   - Split large files by functionality, not arbitrarily
   - Extract test code to separate test modules for files >1,000 lines

2. **Imports**:
   - Group imports: std → external crates → internal modules
   - Use `super::*` for route handlers to access common utilities

3. **Error Handling**:
   - All endpoints return `ApiResponse<T>`
   - Use Sentry for production error tracking
   - RPC fallback for blockchain operations

4. **Transaction Management**:
   - Use `execute_transaction_serialized()` for all blockchain writes
   - Implement RPC fallback for critical operations
   - Handle nonce errors gracefully

## Architecture Evolution

Completed modularization (as of Oct 2024):

1. ✅ **Phase 1**: Extracted test code to `tests/` directory
2. ✅ **Phase 2**: Created `services/` module for blockchain logic
3. ✅ **Phase 3**: Split into `services/beacon/`, `services/perp/`, `services/transaction/`
4. ✅ **Phase 4**: Organized into `models/` module (app_state, requests, responses)

The codebase now follows a clean layered architecture:
- **Routes**: HTTP handlers (thin layer)
- **Services**: Business logic (core operations)
- **Models**: Data structures and types
- **Guards/Fairings**: Cross-cutting concerns

## Performance Considerations

- Tests timeout at 120s by default
- Use `make test-fast` for quick feedback during development
- Anvil cleanup runs between test categories to prevent resource exhaustion
- Multicall3 batching for operations on multiple contracts

## For Junior Engineers

When making changes:

1. **Small changes**: Edit existing files directly
2. **New endpoints**: Add to relevant route file
3. **New features**: Create new file in routes/
4. **Large features**: Consult with senior engineer on structure

Always run before committing:
- `make fmt` - Format code
- `make test-fast` - Quick tests
- `make lint` - Check for issues