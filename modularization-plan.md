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
└── test_fixtures/             # Test utilities and fixtures ✓ (JSON ABIs, mocks remain here)
```

### Final Test Structure

After execution, the test organization was simplified to a flat structure under the standard `tests/` directory at the crate root, with clear separation between unit and integration tests:

```
tests/
├── unit_tests/                # Module-specific unit tests (isolated logic)
│   ├── beacon_core_tests.rs   # Core beacon operations (create, register, validation)
│   ├── beacon_verifiable_tests.rs # Verifiable beacon operations (ZK proof handling)
│   ├── beacon_batch_tests.rs  # Batch and multicall beacon operations
│   ├── perp_operations_tests.rs # Perp deployment, liquidity, batch operations
│   └── transaction_events_tests.rs # Event parsing utilities (all parse_* functions)
└── integration_tests/         # End-to-end cross-module flows
    ├── nonce_sync_tests.rs    # Nonce management and RPC fallback integration
    ├── event_verification_integration.rs # Full create → update → event parse flows
    └── full_flow_integration.rs # Complete perp/beacon E2E (deploy → deposit → verify)
```

**test_fixtures/** remains unchanged at the crate root for shared resources:
- JSON ABI files (Beacon.json, PerpHook.json, etc.)
- Mock contracts and Anvil utilities (TestUtils, AnvilManager)

This flat structure avoids deep nesting while maintaining separation: unit_tests/ for isolated module testing, integration_tests/ for cross-service flows (e.g., event verification across beacon/perp).

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
- Tests colocated in standard `tests/` directory at crate root
- **unit_tests/**: Isolated module tests (e.g., individual service functions, event parsing)
- **integration_tests/**: Cross-module E2E flows (e.g., nonce sync, full event verification)
- Shared mocks/utilities in `test_fixtures/` (JSON ABIs, AnvilManager)
- No tests in routes files—pure HTTP handlers
- Run with `cargo test unit_tests/` or `cargo test integration_tests/` for targeted execution

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

## Parallelizable Implementation Tasks

To enable parallel development across multiple agents, the implementation is broken into independent tasks with clear dependencies. Agents can work simultaneously on non-dependent tasks. Total estimated effort: ~3-4 weeks with 3-4 agents.

### Task Coordination Guidelines
- **Dependency Management**: Use git branches for each task (e.g., `task/models-split`). Merge foundational tasks first, then integrate domain-specific ones.
- **Parallel Groups**: Tasks in the same group have no inter-dependencies and can be worked on concurrently.
- **Integration Points**: After completing a group, run `make quality` and full tests. Use PRs for review before merging.
- **Communication**: Coordinate via shared issues/PRs for cross-task concerns (e.g., import patterns).
- **Priorities**: Foundational tasks (events, models) first for quick wins and to unblock domain tasks.

### Foundational Tasks (Parallel Group 1: ~4-6 days total, 2 agents)
These must be completed first as they provide shared infrastructure.

1. **Task: Extract and Centralize Event Parsing** (Effort: 1-2 days, Agent: Events Specialist)
   - Create `src/services/transaction/events.rs` (~450 lines) and move all `parse_*` functions (including new `parse_data_updated_event`)
   - Add comprehensive tests to `tests/unit_tests/transaction_events_tests.rs`
   - Update existing code to import from new module
   - **Dependencies**: None
   - **Output**: Centralized event verification ready for services
   - **Risk**: Low—mostly extraction

2. **Task: Split Models Module** (Effort: 1-2 days, Agent: Data/Model Specialist)
   - Create `src/models/` directory: `requests.rs` (~250 lines), `responses.rs` (~200 lines), `app_state.rs` (~195 lines including endpoints)
   - Update all imports across codebase to use new structure
   - Add module tests for serialization/validation
   - **Dependencies**: None (independent of events)
   - **Output**: Organized data models, reducing models.rs bloat
   - **Risk**: Medium—import updates may need careful testing

### Domain Modularization Tasks (Parallel Group 2: ~10-14 days total, 2-3 agents)
These can start after Group 1, but beacon and perp tasks can run in parallel.

3. **Task: Modularize Beacon Services** (Effort: 4-5 days, Agent: Beacon Specialist)
   - Create `src/services/beacon/`: `core.rs` (~600 lines: core ops + registry), `verifiable.rs` (~450 lines), `batch.rs` (~800 lines: batch + multicall)
   - Refactor `routes/beacon.rs` to thin handlers (~450 lines) delegating to services
   - Integrate event parsing calls from `events.rs` (e.g., in update_beacon flow)
   - Move/update tests: `unit_tests/beacon_core_tests.rs`, `unit_tests/beacon_verifiable_tests.rs`, `unit_tests/beacon_batch_tests.rs`
   - **Dependencies**: Events extraction (Task 1), Models split (Task 2)
   - **Output**: Beacon.rs reduced from 2931 to ~450 lines
   - **Risk**: High—complex business logic, thorough testing needed

4. **Task: Modularize Perp Services** (Effort: 3-4 days, Agent: Perp Specialist)
   - Create `src/services/perp/`: `operations.rs` (~900 lines: deployment + liquidity + batch)
   - Refactor `routes/perp.rs` to thin handlers (~350 lines) delegating to services
   - Update to use `events.rs` for PerpCreated/MakerPositionOpened parsing
   - Move/update tests to `unit_tests/perp_operations_tests.rs`
   - **Dependencies**: Events extraction (Task 1), Models split (Task 2)
   - **Output**: Perp.rs reduced from 3351 to ~350 lines
   - **Risk**: Medium—similar to beacon but less complex

### Utility Services Tasks (Parallel Group 3: ~3-4 days total, 1 agent)
These can overlap with Group 2 once events are done.

5. **Task: Implement Transaction Execution Services** (Effort: 1-2 days, Agent: Transaction Specialist)
   - Create `src/services/transaction/execution.rs` (~250 lines: execute_transaction_serialized, RPC fallback, nonce mgmt)
   - Update beacon/perp services to use it
   - Add unit tests for fallback/nonce logic in `unit_tests/transaction_events_tests.rs`
   - **Dependencies**: Events extraction (Task 1)
   - **Output**: Centralized transaction utils
   - **Risk**: Low—extraction from existing code

6. **Task: Implement Multicall Services** (Effort: 1-2 days, Agent: Transaction Specialist)
   - Create `src/services/transaction/multicall.rs` (~350 lines: generic multicall3 utils)
   - Refactor batch.rs in beacon/perp to use shared multicall
   - Add tests for atomic batching in `unit_tests/beacon_batch_tests.rs` and `unit_tests/perp_operations_tests.rs`
   - **Dependencies**: Events extraction (Task 1), Beacon/Perp modularization (Tasks 3-4)
   - **Output**: Reusable multicall logic
   - **Risk**: Medium—integration with batches

### Integration and Cleanup Tasks (Sequential Group 4: ~2-3 days total, 1-2 agents)
These finalize after all parallels.

7. **Task: Test Reorganization and Integration** (Effort: 1-2 days, Agent: Testing Specialist)
   - Organize tests into flat `tests/unit_tests/` (module-specific) and `tests/integration_tests/` (E2E flows)
   - Move remaining tests to new structure (e.g., integrate event tests into unit_tests/transaction_events_tests.rs)
   - Add end-to-end integration tests for event verification flows in integration_tests/
   - Ensure 100% test pass rate with new imports
   - **Dependencies**: All previous tasks
   - **Output**: Fully tested modular codebase with flat test structure
   - **Risk**: Low—mostly reorganization

8. **Task: Final Integration, Documentation, and Quality** (Effort: 1 day, Agent: Lead)
   - Run full `make quality`, fix any lint/import issues
   - Update README/docs with new structure, including flat tests/ organization
   - Performance verification (compilation time)
   - Merge all branches
   - **Dependencies**: All previous tasks
   - **Output**: Production-ready modular codebase
   - **Risk**: Low—final polish

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
- Extensive testing at each task completion, especially event parsing
- Incremental git branches per task
- Maintain backward compatibility during integration

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

### Risk: Parallel Coordination
**Mitigation**:
- Clear dependency graph (tasks list above)
- Regular sync points after each group
- Use PRs for cross-review before merges
- Assign agents to complementary tasks (e.g., one on events, one on models)

## Conclusion

This updated modularization plan balances modularity with practicality by allowing files up to 1000 lines, reducing the number of small files while maintaining clear separation of concerns. The parallelizable task structure enables multiple agents to work efficiently: foundational tasks first (1-2 days parallel), then domain-specific modularization (beacon/perp in parallel, 3-5 days each), utilities overlapping, and final integration. It prioritizes centralizing event parsing (critical for recent verification additions) for quick wins. This approach minimizes total time to ~3 weeks with 3 agents while ensuring a robust, maintainable codebase.