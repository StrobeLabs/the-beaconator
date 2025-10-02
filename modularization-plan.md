# Modularization Plan

## Overview

This document outlines a comprehensive plan to modularize The Beaconator codebase to address the current issue of oversized files (2,000+ lines) that are difficult to maintain and test. The plan has been updated to allow files up to 1000 lines, reducing fragmentation while maintaining modularity.

## Current State Analysis

### Recent Changes

**Event Verification Integration**: The codebase has been recently updated to include event verification for beacon updates:

- **New DataUpdated Event Parsing**: Added `parse_data_updated_event()` function to verify that beacon update transactions actually emit the expected `DataUpdated` event
- **Enhanced Update Flow**: The `update_beacon` endpoint now includes event verification as part of the success criteria
- **Contract Interface Updates**: The `IBeacon` interface now includes the `DataUpdated` event definition
- **Comprehensive Event Coverage**: All major operations now include event parsing for verification:
  - Beacon creation: `BeaconCreated` event
  - Beacon updates: `DataUpdated` event
  - Perp deployment: `PerpCreated` event
  - Liquidity deposits: `MakerPositionOpened` event
- **Test Coverage**: Added dedicated test module `event_parsing_tests` to verify event parsing functionality

### Problematic Large Files

- **`src/routes/beacon.rs`** (2,931 lines) - Contains beacon creation, registration, updates, and batch operations (increased due to event parsing additions)
- **`src/routes/perp.rs`** (3,351 lines) - Contains perp deployment, liquidity deposits, and batch operations
- **`src/models.rs`** (645 lines) - Contains all data models, requests/responses, and AppState

### Current File Structure
```
src/
├── main.rs                    (64 lines) ✓
├── lib.rs                     (409 lines) ✓
├── guards.rs                  (66 lines) ✓
├── fairings.rs                (76 lines) ✓
├── models.rs                  (645 lines) ❌ TOO LARGE
├── routes/
│   ├── mod.rs                 (201 lines) ✓
│   ├── info.rs                (146 lines) ✓
│   ├── wallet.rs              (277 lines) ✓
│   ├── beacon.rs              (2,931 lines) ❌ TOO LARGE
│   ├── perp.rs                (3,351 lines) ❌ TOO LARGE
│   ├── test_utils.rs          (504 lines) ✓
│   ├── nonce_sync_tests.rs    (294 lines) ✓
│   └── wallet_test.rs         (323 lines) ✓
└── test_fixtures/             ✓
```

### Responsibilities Identified

**beacon.rs responsibilities:**
- Helper functions (create_beacon_via_factory, register_beacon_with_registry)
- Event parsing (parse_beacon_created_event, parse_data_updated_event, parse_beacon_created_events_from_multicall)
- Single operations (create_perpcity_beacon, update_beacon, register_beacon)
- Batch operations (batch_create_perpcity_beacon, batch_update_beacon)
- Multicall3 operations (batch_update_with_multicall3, batch_create_beacons_with_multicall3)
- Verifiable beacon operations (create_verifiable_beacon, update_verifiable_beacon)
- Event verification for update operations (DataUpdated event parsing integrated into update_beacon)
- Extensive test suite (1000+ lines) including event parsing tests

**perp.rs responsibilities:**
- Helper functions (deploy_perp_for_beacon, parse_perp_created_event, parse_maker_position_opened_event)
- Single operations (deploy_perp_for_beacon_endpoint, deposit_liquidity_for_perp_endpoint)
- Batch operations (batch_deposit_liquidity_for_perps)
- Multicall3 operations (batch_deposit_liquidity_with_multicall3)
- Error handling utilities (try_decode_revert_reason)
- Event verification for deployment operations (PerpCreated and MakerPositionOpened event parsing)
- Test suite (1000+ lines)

## Proposed Modular Structure

### New Directory Structure

```
src/
├── main.rs                    # Entry point (64 lines) ✓
├── lib.rs                     # Provider setup, ABI loading (409 lines) ✓
├── guards.rs                  # Authentication (66 lines) ✓
├── fairings.rs                # Request/response fairings (76 lines) ✓
├── models/                    # Split models.rs (645 lines)
│   ├── mod.rs                 # Re-exports
│   ├── requests.rs            # All request models (~250 lines)
│   ├── responses.rs           # All response models (~200 lines)
│   └── app_state.rs           # AppState and related types (~195 lines)
├── services/                  # Business logic layer
│   ├── mod.rs                 # Service re-exports
│   ├── beacon/
│   │   ├── mod.rs             # Beacon service re-exports
│   │   ├── core.rs            # Core beacon operations including registry (~600 lines)
│   │   ├── verifiable.rs      # Verifiable beacon operations (~450 lines)
│   │   └── batch.rs           # Batch beacon operations including multicall (~800 lines)
│   ├── perp/
│   │   ├── mod.rs             # Perp service re-exports
│   │   └── operations.rs      # Perp deployment, liquidity, and batch operations (~900 lines)
│   └── transaction/
│       ├── mod.rs             # Transaction utilities re-exports
│       ├── execution.rs       # Transaction execution & fallback (~250 lines)
│       ├── multicall.rs       # Multicall3 operations (~350 lines)
│       └── events.rs          # Centralized event parsing utilities (~450 lines including all parse_* functions)
├── routes/                    # API endpoint handlers (thin layer)
│   ├── mod.rs                 # Shared utilities (201 lines) - keep as is
│   ├── info.rs                # API info endpoints (146 lines) ✓
│   ├── wallet.rs              # Wallet operations (277 lines) ✓
│   ├── beacon.rs              # Beacon route handlers (~450 lines)
│   └── perp.rs                # Perp route handlers (~350 lines)
└── test_fixtures/             # Test utilities and fixtures ✓
    └── tests/                 # Reorganized test files
        ├── beacon/
        │   ├── core_tests.rs
        │   ├── verifiable_tests.rs
        │   └── batch_tests.rs
        ├── perp/
        │   └── operations_tests.rs
        ├── transaction/
        │   └── events_tests.rs  # Comprehensive event parsing tests
        └── integration/
            ├── nonce_sync_tests.rs
            └── wallet_tests.rs
```

### Design Principles

#### 1. Separation of Concerns
- **Routes**: Handle HTTP concerns (parsing, validation, authentication)
- **Services**: Contain business logic and blockchain interactions
- **Models**: Data structures organized by purpose

#### 2. Relaxed Single Responsibility
- Target <1000 lines per file to balance modularity with practicality
- Combine related functionality to avoid excessive small files
- Focus on logical groupings rather than micro-modules

#### 3. Dependency Flow
- Routes → Services → Models
- Services can call other services
- No circular dependencies
- Clear, unidirectional data flow

#### 4. Test Organization
- Tests colocated with the code they test
- Integration tests separated from unit tests
- Shared test utilities in test_fixtures
- Comprehensive event parsing tests centralized

## Module Breakdown

### Models Module (`src/models/`)

**`requests.rs`** (~250 lines):
- All request structs (CreateBeaconRequest, BatchCreatePerpcityBeaconRequest, etc.)
- Request validation logic
- Serde implementations

**`responses.rs`** (~200 lines):
- All response structs (ApiResponse, DeployPerpForBeaconResponse, etc.)
- Success/error response handling
- Serde implementations

**`app_state.rs`** (~195 lines):
- AppState definition
- Provider management
- Configuration types
- EndpointInfo and ApiEndpoints

### Services Module (`src/services/`)

**Beacon Services (`src/services/beacon/`)**:

- **`core.rs`** (~600 lines): Core beacon operations including registry
  - `create_beacon_via_factory()`
  - `register_beacon_with_registry()`
  - Single beacon operations
  - Event verification integration (uses events.rs for DataUpdated parsing)

- **`verifiable.rs`** (~450 lines): Verifiable beacon operations
  - `create_verifiable_beacon()`
  - `update_verifiable_beacon()`
  - ZK proof handling

- **`batch.rs`** (~800 lines): Batch beacon operations including multicall
  - `batch_create_perpcity_beacon()`
  - `batch_update_beacon()`
  - Multicall3 batching logic

**Perp Services (`src/services/perp/`)**:

- **`operations.rs`** (~900 lines): Perp deployment, liquidity, and batch operations
  - `deploy_perp_for_beacon()`
  - `deposit_liquidity_for_perp()`
  - `batch_deposit_liquidity_for_perps()`
  - Event verification using events.rs

**Transaction Services (`src/services/transaction/`)**:

- **`execution.rs`** (~250 lines): Transaction execution & fallback
  - `execute_transaction_serialized()`
  - RPC fallback logic
  - Nonce management

- **`multicall.rs`** (~350 lines): Multicall3 operations
  - Generic multicall utilities (can be shared between beacon and perp)

- **`events.rs`** (~450 lines): Centralized event parsing utilities
  - `parse_beacon_created_event()`
  - `parse_data_updated_event()` (NEW - for beacon update verification)
  - `parse_perp_created_event()`
  - `parse_maker_position_opened_event()`
  - `parse_beacon_created_events_from_multicall()`
  - Log decoding utilities and error handling
  - Comprehensive test coverage for all event types

### Routes Module (`src/routes/`)

**`beacon.rs`** (~450 lines): Beacon route handlers
- Thin HTTP handlers that delegate to beacon services
- Request/response transformation
- Authentication and validation

**`perp.rs`** (~350 lines): Perp route handlers
- Thin HTTP handlers that delegate to perp services
- Request/response transformation
- Authentication and validation

## Implementation Priorities

Given the current codebase sizes, prioritize the following:

1. **High Priority (Week 1)**: Extract event parsing to `src/services/transaction/events.rs` (~450 lines)
   - Centralizes all parse_* functions including the new DataUpdated parsing
   - Reduces duplication and ensures consistent event verification
   - Add comprehensive tests in events_tests.rs

2. **High Priority (Week 1-2)**: Split models.rs into models/ directory
   - Quick win with minimal risk
   - Improves data structure organization

3. **Medium Priority (Week 2-3)**: Modularize beacon.rs
   - Extract to services/beacon/ with core.rs (600 lines), verifiable.rs (450 lines), batch.rs (800 lines)
   - Move route handlers to thin beacon.rs (450 lines)
   - Integrate event verification calls to events.rs

4. **Medium Priority (Week 3-4)**: Modularize perp.rs
   - Extract to services/perp/operations.rs (900 lines)
   - Move route handlers to thin perp.rs (350 lines)
   - Update to use centralized event parsing

5. **Low Priority (Week 4)**: Test reorganization and integration testing
   - Move tests to new structure
   - Add end-to-end tests for event verification flows

## Migration Benefits

### 1. Maintainability
- **Easier Navigation**: Specific functionality is easy to find
- **Focused Changes**: Modifications affect smaller, focused files (under 1000 lines)
- **Clear Ownership**: Each module has a clear purpose and responsibility

### 2. Testability
- **Isolated Testing**: Components can be unit tested in isolation
- **Mocking**: Service layer can be easily mocked for route testing
- **Test Organization**: Tests are organized alongside the code they test
- **Event Verification Testing**: Centralized event tests ensure consistent coverage

### 3. Team Development
- **Parallel Development**: Multiple developers can work on different areas without conflicts
- **Code Reviews**: Smaller files make code reviews more focused and effective
- **Onboarding**: New team members can understand specific areas more easily

### 4. Code Reuse
- **Service Composition**: Services can be reused across different routes
- **Utility Functions**: Common functionality (event parsing, transaction execution) is centralized
- **Business Logic**: Core logic is separated from HTTP concerns

### 5. Performance
- **Faster Compilation**: Smaller compilation units compile faster
- **Incremental Builds**: Changes to one module don't require recompiling everything
- **Better IDE Support**: Smaller files improve IDE performance and navigation

## Updated Implementation Considerations

### Event Verification Impact

The recent addition of comprehensive event verification affects the modularization strategy:

1. **Event Parsing Centralization**: All event parsing functions are centralized in `src/services/transaction/events.rs` to avoid duplication and ensure consistent error handling across beacon and perp operations
2. **Service Layer Integration**: Event verification is a core part of business logic and handled in the service layer, with routes only handling HTTP transformation
3. **Testing Strategy**: Dedicated `events_tests.rs` module with comprehensive coverage for all event types (BeaconCreated, DataUpdated, PerpCreated, etc.)
4. **Error Handling**: Event parsing failures treated as business logic errors in services, with appropriate HTTP status mapping in routes

### Contract Interface Dependencies

The modularization must account for:
- Shared contract interfaces defined in `src/routes/mod.rs` using `sol!` macro
- Event type definitions used across services (imported from routes/mod.rs)
- ABI loading and contract instantiation patterns (centralized in app_state.rs)

## Implementation Strategy

### Phase 1: Extract Models and Events (~3-4 days)
1. Create `src/models/` directory structure and split models.rs
2. Create `src/services/transaction/events.rs` and move all parse_* functions
3. Update imports and ensure event verification still works
4. Add comprehensive tests in events_tests.rs
5. Run full test suite

### Phase 2: Beacon Modularization (~5-7 days)
1. Create `src/services/beacon/` structure
2. Extract core operations to core.rs (including registry)
3. Extract verifiable operations to verifiable.rs
4. Extract batch/multicall to batch.rs
5. Refactor routes/beacon.rs to thin handlers
6. Update tests accordingly

### Phase 3: Perp Modularization (~4-5 days)
1. Create `src/services/perp/` with operations.rs
2. Extract deployment, liquidity, and batch logic
3. Refactor routes/perp.rs to thin handlers
4. Update tests to new structure

### Phase 4: Transaction Services (~2-3 days)
1. Create execution.rs and multicall.rs
2. Ensure services use these utilities
3. Final test reorganization

### Phase 5: Integration and Cleanup (~2 days)
1. Full integration testing
2. Performance verification
3. Documentation updates
4. Final quality checks

## Success Metrics

### Quantitative Goals
- No single file over 1000 lines
- Test coverage maintained or improved (target 80%+)
- Compilation time improved by 15%+
- Zero functional regressions, especially in event verification

### Qualitative Goals
- Improved code readability and navigation
- Easier onboarding for new developers
- More focused and effective code reviews
- Better separation of concerns with practical file sizes

## Risks and Mitigation

### Risk: Breaking Changes
**Mitigation**:
- Extensive testing at each phase, especially event parsing
- Incremental migration approach
- Maintain backward compatibility during transition

### Risk: Import Complexity
**Mitigation**:
- Clear module re-export strategy in mod.rs files
- Consistent import patterns (use services::* where appropriate)
- Documentation of module boundaries and dependencies

### Risk: Over-Engineering
**Mitigation**:
- Focus on current pain points (beacon.rs and perp.rs)
- Relaxed file size limits to avoid micro-modules
- Keep related functionality together (e.g., all perp ops in one file)
- Prioritize event centralization for immediate benefits

## Conclusion

This updated modularization plan balances modularity with practicality by allowing files up to 1000 lines, reducing the number of small files while maintaining clear separation of concerns. The structure prioritizes centralizing event parsing (critical for recent verification additions) and focuses on the largest pain points first. The incremental approach ensures minimal disruption while delivering immediate benefits in code organization, maintainability, and testability.