// Transaction execution tests - extracted from src/services/transaction/execution.rs
//
// Note: The global transaction serializer has been removed.
// Transaction serialization is now handled by Redis-based distributed locks
// in the wallet module. See `WalletLock` for details.

use the_beaconator::services::transaction::execution::is_nonce_error;

#[test]
fn test_is_nonce_error_detection() {
    // Test various nonce error patterns
    assert!(is_nonce_error("nonce too low"));
    assert!(is_nonce_error("NONCE TOO LOW")); // Case insensitive
    assert!(is_nonce_error("Error: nonce too high"));
    assert!(is_nonce_error("invalid nonce"));
    assert!(is_nonce_error("replacement transaction underpriced"));

    // Non-nonce errors should return false
    assert!(!is_nonce_error("insufficient funds"));
    assert!(!is_nonce_error("gas limit exceeded"));
    assert!(!is_nonce_error(""));
}
