# The Beaconator Architecture Guide

## Current Structure

The Beaconator follows a Rocket.rs web service pattern with the following structure:

```
src/
├── main.rs           # Entry point
├── lib.rs           # Core initialization and provider setup
├── models.rs        # Request/response models and AppState
├── guards.rs        # Authentication guards
├── fairings.rs      # Request/response middleware
└── routes/          # HTTP endpoint handlers
    ├── mod.rs       # Route mounting and shared utilities
    ├── beacon.rs    # Beacon operations (2,828 lines)
    ├── perp.rs      # Perpetual operations (3,351 lines)
    ├── wallet.rs    # Wallet funding operations
    └── info.rs      # API documentation endpoints
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
   - Common models → `models.rs`
   - Authentication → `guards.rs`

### Current Module Responsibilities

#### `routes/beacon.rs`
- Beacon creation (factory, registry)
- Beacon updates (single, batch)
- Verifiable beacon operations
- Multicall3 batch operations

#### `routes/perp.rs`
- Perpetual deployment
- Liquidity management
- Batch liquidity operations
- Error decoding and recovery

#### `routes/wallet.rs`
- Guest wallet funding
- USDC and ETH transfers
- Balance checks

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

## Refactoring Roadmap

Future improvements planned:

1. **Phase 1**: Extract test code from large files
2. **Phase 2**: Create services/ module for shared blockchain logic
3. **Phase 3**: Split beacon.rs and perp.rs into submodules
4. **Phase 4**: Move integration tests to tests/ directory

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