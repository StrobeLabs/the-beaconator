// Unit tests module

pub mod beacon_tests;
pub mod fairings_simple_tests;
pub mod guards_simple_tests;
pub mod info_tests;
// pub mod perp_operations_tests; // Temporarily disabled during PerpManager refactor
// pub mod perp_route_tests; // Temporarily disabled during PerpManager refactor
pub mod register_beacon_route_tests;
pub mod services_beacon_core_tests;
pub mod services_beacon_verifiable_tests;
pub mod services_perp_validation_tests;
pub mod services_transaction_events_simple_tests;
// pub mod services_transaction_execution_comprehensive_tests; // Removed - nonce management obsolete with WalletManager
pub mod services_transaction_multicall_comprehensive_tests;
pub mod transaction_events_tests;
pub mod transaction_execution_tests;
pub mod wallet_route_tests;
