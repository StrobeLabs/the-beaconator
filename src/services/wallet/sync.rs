//! Wallet sync service for syncing Turnkey wallets to Redis pool
//!
//! This module provides [`WalletSyncService`] which synchronizes wallets from
//! Turnkey to the Redis wallet pool. It handles adding new wallets while
//! preserving existing wallet state (status and designated beacons).
//!
//! # Example
//!
//! ```rust,ignore
//! use the_beaconator::services::wallet::{TurnkeyWalletAPI, WalletPool, WalletSyncService};
//!
//! let turnkey_api = TurnkeyWalletAPI::new(...)?;
//! let pool = WalletPool::new("redis://localhost:6379", "instance-1").await?;
//! let sync_service = WalletSyncService::new(turnkey_api, pool);
//!
//! let result = sync_service.sync().await?;
//! println!("Added {} wallets, {} unchanged", result.added.len(), result.unchanged.len());
//! ```

use alloy::primitives::Address;

use crate::models::wallet::{WalletInfo, WalletStatus};
use crate::services::wallet::{TurnkeyWalletAPI, WalletPool};

/// Result of a wallet sync operation
///
/// Tracks the outcome of syncing wallets from Turnkey to the Redis pool,
/// categorizing wallets as added, unchanged, or errored.
///
/// # Examples
///
/// ```
/// use alloy::primitives::Address;
/// use the_beaconator::services::wallet::SyncResult;
///
/// let mut result = SyncResult::default();
/// result.added.push(Address::from([0x01; 20]));
/// result.unchanged.push(Address::from([0x02; 20]));
///
/// assert_eq!(result.total_successful(), 2);
/// assert!(!result.has_errors());
/// ```
#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    /// Wallets that were added to the pool
    pub added: Vec<Address>,
    /// Wallets that already existed in the pool (unchanged)
    pub unchanged: Vec<Address>,
    /// Errors encountered during sync (wallet-specific errors)
    pub errors: Vec<String>,
}

impl SyncResult {
    /// Create a new empty SyncResult
    ///
    /// # Examples
    ///
    /// ```
    /// use the_beaconator::services::wallet::SyncResult;
    ///
    /// let result = SyncResult::new();
    /// assert!(result.is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the total number of wallets processed successfully
    ///
    /// This includes both added and unchanged wallets.
    /// Errors are not included in this count.
    ///
    /// # Examples
    ///
    /// ```
    /// use alloy::primitives::Address;
    /// use the_beaconator::services::wallet::SyncResult;
    ///
    /// let mut result = SyncResult::new();
    /// result.added.push(Address::from([0x01; 20]));
    /// result.added.push(Address::from([0x02; 20]));
    /// result.unchanged.push(Address::from([0x03; 20]));
    /// result.errors.push("Error for wallet".to_string());
    ///
    /// // Errors don't count toward successful
    /// assert_eq!(result.total_successful(), 3);
    /// ```
    pub fn total_successful(&self) -> usize {
        self.added.len() + self.unchanged.len()
    }

    /// Check if there are any errors in the sync result
    ///
    /// # Examples
    ///
    /// ```
    /// use the_beaconator::services::wallet::SyncResult;
    ///
    /// let mut result = SyncResult::new();
    /// assert!(!result.has_errors());
    ///
    /// result.errors.push("Failed to sync wallet".to_string());
    /// assert!(result.has_errors());
    /// ```
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Check if the result is completely empty (no wallets processed)
    ///
    /// # Examples
    ///
    /// ```
    /// use alloy::primitives::Address;
    /// use the_beaconator::services::wallet::SyncResult;
    ///
    /// let result = SyncResult::new();
    /// assert!(result.is_empty());
    ///
    /// let mut result_with_data = SyncResult::new();
    /// result_with_data.added.push(Address::from([0x01; 20]));
    /// assert!(!result_with_data.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.unchanged.is_empty() && self.errors.is_empty()
    }

    /// Get the total number of wallets processed (including errors)
    ///
    /// # Examples
    ///
    /// ```
    /// use alloy::primitives::Address;
    /// use the_beaconator::services::wallet::SyncResult;
    ///
    /// let mut result = SyncResult::new();
    /// result.added.push(Address::from([0x01; 20]));
    /// result.errors.push("Error".to_string());
    ///
    /// assert_eq!(result.total_processed(), 2);
    /// ```
    pub fn total_processed(&self) -> usize {
        self.added.len() + self.unchanged.len() + self.errors.len()
    }

    /// Get the success rate as a percentage (0.0 to 100.0)
    ///
    /// Returns 100.0 if no wallets were processed (to avoid division by zero).
    ///
    /// # Examples
    ///
    /// ```
    /// use alloy::primitives::Address;
    /// use the_beaconator::services::wallet::SyncResult;
    ///
    /// let mut result = SyncResult::new();
    /// result.added.push(Address::from([0x01; 20]));
    /// result.added.push(Address::from([0x02; 20]));
    /// result.errors.push("Error".to_string());
    ///
    /// // 2 successful out of 3 total = 66.67%
    /// let rate = result.success_rate();
    /// assert!(rate > 66.0 && rate < 67.0);
    ///
    /// // Empty result has 100% success rate
    /// let empty_result = SyncResult::new();
    /// assert_eq!(empty_result.success_rate(), 100.0);
    /// ```
    pub fn success_rate(&self) -> f64 {
        let total = self.total_processed();
        if total == 0 {
            return 100.0;
        }
        (self.total_successful() as f64 / total as f64) * 100.0
    }
}

/// Service for syncing wallets from Turnkey to Redis pool
pub struct WalletSyncService<'a> {
    turnkey_api: TurnkeyWalletAPI,
    pool: &'a WalletPool,
    /// Optional list of allowed wallet IDs to filter by
    /// If empty, all wallets are synced (not recommended for production)
    allowed_wallet_ids: Vec<String>,
}

impl<'a> WalletSyncService<'a> {
    /// Create a new WalletSyncService
    ///
    /// # Arguments
    ///
    /// * `turnkey_api` - Turnkey API client for listing wallets
    /// * `pool` - Reference to Redis wallet pool for storage
    pub fn new(turnkey_api: TurnkeyWalletAPI, pool: &'a WalletPool) -> Self {
        Self {
            turnkey_api,
            pool,
            allowed_wallet_ids: vec![],
        }
    }

    /// Create a new WalletSyncService with wallet ID filtering
    ///
    /// Only wallets with IDs in `allowed_wallet_ids` will be synced.
    /// This is the recommended approach for production to ensure only
    /// beaconator-specific wallets are added to the pool.
    ///
    /// # Arguments
    ///
    /// * `turnkey_api` - Turnkey API client for listing wallets
    /// * `pool` - Reference to Redis wallet pool for storage
    /// * `allowed_wallet_ids` - List of Turnkey wallet IDs to sync
    pub fn with_allowed_wallet_ids(
        turnkey_api: TurnkeyWalletAPI,
        pool: &'a WalletPool,
        allowed_wallet_ids: Vec<String>,
    ) -> Self {
        Self {
            turnkey_api,
            pool,
            allowed_wallet_ids,
        }
    }

    /// Sync wallets from Turnkey to the Redis pool
    ///
    /// This method:
    /// 1. Fetches all Ethereum wallets from Turnkey
    /// 2. Filters to only allowed wallet IDs (if configured)
    /// 3. For each wallet, checks if it exists in Redis
    /// 4. If not exists, adds it with Available status
    /// 5. If exists, skips it to preserve existing status and designated_beacons
    ///
    /// # Returns
    ///
    /// A [`SyncResult`] containing counts of added, unchanged, and errored wallets.
    ///
    /// # Errors
    ///
    /// Returns an error if fetching wallets from Turnkey fails.
    /// Individual wallet errors are collected in the result's `errors` field.
    pub async fn sync(&self) -> Result<SyncResult, String> {
        tracing::info!("Starting wallet sync from Turnkey to Redis pool");

        // Fetch all wallets from Turnkey
        let turnkey_wallets = self
            .turnkey_api
            .list_wallet_accounts()
            .await
            .map_err(|e| format!("Failed to fetch wallets from Turnkey: {e}"))?;

        tracing::info!(
            wallet_count = turnkey_wallets.len(),
            "Fetched wallets from Turnkey"
        );

        // Filter by allowed wallet IDs if configured
        let wallets_to_sync: Vec<_> = if self.allowed_wallet_ids.is_empty() {
            tracing::warn!(
                "No allowed_wallet_ids configured - syncing ALL wallets from Turnkey. \
                 This is not recommended for production. Set BEACONATOR_WALLET_IDS to restrict."
            );
            turnkey_wallets
        } else {
            let filtered: Vec<_> = turnkey_wallets
                .into_iter()
                .filter(|w| self.allowed_wallet_ids.contains(&w.wallet_id))
                .collect();

            tracing::info!(
                filtered_count = filtered.len(),
                allowed_ids = ?self.allowed_wallet_ids,
                "Filtered to allowed wallet IDs"
            );

            filtered
        };

        let mut result = SyncResult::new();

        for wallet in wallets_to_sync {
            let address = wallet.address;
            let wallet_id = wallet.wallet_id.clone();

            match self.sync_single_wallet(address, wallet_id).await {
                Ok(was_added) => {
                    if was_added {
                        result.added.push(address);
                    } else {
                        result.unchanged.push(address);
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        address = %address,
                        error = %e,
                        "Failed to sync wallet"
                    );
                    result.errors.push(format!("Wallet {address}: {e}"));
                }
            }
        }

        tracing::info!(
            added = result.added.len(),
            unchanged = result.unchanged.len(),
            errors = result.errors.len(),
            "Wallet sync completed"
        );

        Ok(result)
    }

    /// Sync a single wallet to the pool
    ///
    /// Returns `Ok(true)` if the wallet was added, `Ok(false)` if it already existed.
    async fn sync_single_wallet(
        &self,
        address: Address,
        turnkey_key_id: String,
    ) -> Result<bool, String> {
        // Explicitly check if wallet exists using wallet_exists
        // This properly distinguishes between "not found" and actual Redis errors
        let exists = self.pool.wallet_exists(&address).await?;

        if exists {
            // Wallet exists, preserve existing state
            tracing::debug!(
                address = %address,
                "Wallet already exists in pool, skipping"
            );
            Ok(false)
        } else {
            // Wallet doesn't exist, add it
            let info = WalletInfo {
                address,
                turnkey_key_id,
                status: WalletStatus::Available,
                designated_beacons: vec![],
            };

            self.pool.add_wallet(info).await?;

            tracing::info!(
                address = %address,
                "Added new wallet to pool"
            );

            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create test addresses
    fn test_address(byte: u8) -> Address {
        Address::from([byte; 20])
    }

    // ========================================
    // SyncResult::new() tests
    // ========================================

    #[test]
    fn test_sync_result_new() {
        let result = SyncResult::new();
        assert!(result.added.is_empty());
        assert!(result.unchanged.is_empty());
        assert!(result.errors.is_empty());
        assert_eq!(result.total_successful(), 0);
    }

    #[test]
    fn test_sync_result_default() {
        let result = SyncResult::default();
        assert!(result.added.is_empty());
        assert!(result.unchanged.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_sync_result_new_equals_default() {
        let new_result = SyncResult::new();
        let default_result = SyncResult::default();

        assert_eq!(new_result.added.len(), default_result.added.len());
        assert_eq!(new_result.unchanged.len(), default_result.unchanged.len());
        assert_eq!(new_result.errors.len(), default_result.errors.len());
    }

    // ========================================
    // SyncResult::is_empty() tests
    // ========================================

    #[test]
    fn test_sync_result_empty_result() {
        let result = SyncResult::new();
        assert!(result.is_empty());
        assert_eq!(result.total_successful(), 0);
        assert_eq!(result.total_processed(), 0);
        assert!(!result.has_errors());
    }

    #[test]
    fn test_sync_result_not_empty_with_added() {
        let mut result = SyncResult::new();
        result.added.push(test_address(0x01));
        assert!(!result.is_empty());
    }

    #[test]
    fn test_sync_result_not_empty_with_unchanged() {
        let mut result = SyncResult::new();
        result.unchanged.push(test_address(0x01));
        assert!(!result.is_empty());
    }

    #[test]
    fn test_sync_result_not_empty_with_errors() {
        let mut result = SyncResult::new();
        result.errors.push("error".to_string());
        assert!(!result.is_empty());
    }

    // ========================================
    // SyncResult::total_successful() tests
    // ========================================

    #[test]
    fn test_sync_result_total_successful() {
        let mut result = SyncResult::new();
        result.added.push(test_address(0x01));
        result.added.push(test_address(0x02));
        result.unchanged.push(test_address(0x03));
        result.errors.push("some error".to_string());

        assert_eq!(result.total_successful(), 3);
    }

    #[test]
    fn test_sync_result_total_successful_only_added() {
        let mut result = SyncResult::new();
        result.added.push(test_address(0x01));
        result.added.push(test_address(0x02));
        result.added.push(test_address(0x03));

        assert_eq!(result.total_successful(), 3);
    }

    #[test]
    fn test_sync_result_total_successful_only_unchanged() {
        let mut result = SyncResult::new();
        result.unchanged.push(test_address(0x01));
        result.unchanged.push(test_address(0x02));

        assert_eq!(result.total_successful(), 2);
    }

    #[test]
    fn test_sync_result_total_successful_excludes_errors() {
        let mut result = SyncResult::new();
        result.errors.push("error 1".to_string());
        result.errors.push("error 2".to_string());
        result.errors.push("error 3".to_string());

        assert_eq!(result.total_successful(), 0);
    }

    // ========================================
    // SyncResult::has_errors() tests
    // ========================================

    #[test]
    fn test_sync_result_has_errors_empty() {
        let result = SyncResult::new();
        assert!(!result.has_errors());
    }

    #[test]
    fn test_sync_result_has_errors_with_single_error() {
        let mut result = SyncResult::new();
        result.errors.push("Failed to sync wallet".to_string());
        assert!(result.has_errors());
    }

    #[test]
    fn test_sync_result_has_errors_with_multiple_errors() {
        let mut result = SyncResult::new();
        result.errors.push("Error 1".to_string());
        result.errors.push("Error 2".to_string());
        result.errors.push("Error 3".to_string());
        assert!(result.has_errors());
    }

    #[test]
    fn test_sync_result_has_errors_with_success_and_errors() {
        let mut result = SyncResult::new();
        result.added.push(test_address(0x01));
        result.unchanged.push(test_address(0x02));
        result.errors.push("Some error".to_string());
        assert!(result.has_errors());
    }

    // ========================================
    // SyncResult::total_processed() tests
    // ========================================

    #[test]
    fn test_sync_result_total_processed_empty() {
        let result = SyncResult::new();
        assert_eq!(result.total_processed(), 0);
    }

    #[test]
    fn test_sync_result_total_processed_mixed() {
        let mut result = SyncResult::new();
        result.added.push(test_address(0x01));
        result.added.push(test_address(0x02));
        result.unchanged.push(test_address(0x03));
        result.errors.push("error".to_string());

        assert_eq!(result.total_processed(), 4);
    }

    #[test]
    fn test_sync_result_total_processed_only_errors() {
        let mut result = SyncResult::new();
        result.errors.push("error 1".to_string());
        result.errors.push("error 2".to_string());

        assert_eq!(result.total_processed(), 2);
    }

    // ========================================
    // SyncResult::success_rate() tests
    // ========================================

    #[test]
    fn test_sync_result_success_rate_empty() {
        let result = SyncResult::new();
        assert_eq!(result.success_rate(), 100.0);
    }

    #[test]
    fn test_sync_result_success_rate_all_successful() {
        let mut result = SyncResult::new();
        result.added.push(test_address(0x01));
        result.unchanged.push(test_address(0x02));

        assert_eq!(result.success_rate(), 100.0);
    }

    #[test]
    fn test_sync_result_success_rate_all_errors() {
        let mut result = SyncResult::new();
        result.errors.push("error 1".to_string());
        result.errors.push("error 2".to_string());

        assert_eq!(result.success_rate(), 0.0);
    }

    #[test]
    fn test_sync_result_success_rate_mixed() {
        let mut result = SyncResult::new();
        result.added.push(test_address(0x01));
        result.added.push(test_address(0x02));
        result.errors.push("error".to_string());

        // 2 successful out of 3 total = 66.666...%
        let rate = result.success_rate();
        assert!(rate > 66.66 && rate < 66.67);
    }

    #[test]
    fn test_sync_result_success_rate_half() {
        let mut result = SyncResult::new();
        result.added.push(test_address(0x01));
        result.errors.push("error".to_string());

        assert_eq!(result.success_rate(), 50.0);
    }

    #[test]
    fn test_sync_result_success_rate_quarter() {
        let mut result = SyncResult::new();
        result.added.push(test_address(0x01));
        result.errors.push("error 1".to_string());
        result.errors.push("error 2".to_string());
        result.errors.push("error 3".to_string());

        assert_eq!(result.success_rate(), 25.0);
    }

    // ========================================
    // SyncResult edge case tests
    // ========================================

    #[test]
    fn test_sync_result_only_errors() {
        let mut result = SyncResult::new();
        result.errors.push("Error for wallet 0x01".to_string());
        result.errors.push("Error for wallet 0x02".to_string());
        result.errors.push("Network timeout".to_string());

        assert!(!result.is_empty());
        assert_eq!(result.total_successful(), 0);
        assert_eq!(result.total_processed(), 3);
        assert!(result.has_errors());
        assert_eq!(result.success_rate(), 0.0);
        assert_eq!(result.errors.len(), 3);
    }

    #[test]
    fn test_sync_result_mixed_added_unchanged_errors() {
        let mut result = SyncResult::new();

        // Add 3 wallets
        result.added.push(test_address(0x01));
        result.added.push(test_address(0x02));
        result.added.push(test_address(0x03));

        // 2 unchanged
        result.unchanged.push(test_address(0x04));
        result.unchanged.push(test_address(0x05));

        // 5 errors
        result.errors.push("Error 1".to_string());
        result.errors.push("Error 2".to_string());
        result.errors.push("Error 3".to_string());
        result.errors.push("Error 4".to_string());
        result.errors.push("Error 5".to_string());

        assert_eq!(result.added.len(), 3);
        assert_eq!(result.unchanged.len(), 2);
        assert_eq!(result.errors.len(), 5);
        assert_eq!(result.total_successful(), 5);
        assert_eq!(result.total_processed(), 10);
        assert!(result.has_errors());
        assert!(!result.is_empty());
        assert_eq!(result.success_rate(), 50.0);
    }

    #[test]
    fn test_sync_result_only_added() {
        let mut result = SyncResult::new();
        result.added.push(test_address(0x01));
        result.added.push(test_address(0x02));

        assert_eq!(result.added.len(), 2);
        assert!(result.unchanged.is_empty());
        assert!(!result.has_errors());
        assert_eq!(result.total_successful(), 2);
        assert_eq!(result.success_rate(), 100.0);
    }

    #[test]
    fn test_sync_result_only_unchanged() {
        let mut result = SyncResult::new();
        result.unchanged.push(test_address(0x01));
        result.unchanged.push(test_address(0x02));
        result.unchanged.push(test_address(0x03));

        assert!(result.added.is_empty());
        assert_eq!(result.unchanged.len(), 3);
        assert!(!result.has_errors());
        assert_eq!(result.total_successful(), 3);
        assert_eq!(result.success_rate(), 100.0);
    }

    // ========================================
    // SyncResult Clone and Debug trait tests
    // ========================================

    #[test]
    fn test_sync_result_clone() {
        let mut result = SyncResult::new();
        result.added.push(test_address(0x01));
        result.unchanged.push(test_address(0x02));
        result.errors.push("error".to_string());

        let cloned = result.clone();

        assert_eq!(cloned.added.len(), result.added.len());
        assert_eq!(cloned.unchanged.len(), result.unchanged.len());
        assert_eq!(cloned.errors.len(), result.errors.len());
        assert_eq!(cloned.added[0], result.added[0]);
        assert_eq!(cloned.unchanged[0], result.unchanged[0]);
        assert_eq!(cloned.errors[0], result.errors[0]);
    }

    #[test]
    fn test_sync_result_debug() {
        let result = SyncResult::new();
        let debug_str = format!("{result:?}");
        assert!(debug_str.contains("SyncResult"));
        assert!(debug_str.contains("added"));
        assert!(debug_str.contains("unchanged"));
        assert!(debug_str.contains("errors"));
    }

    // ========================================
    // SyncResult with large numbers of wallets
    // ========================================

    #[test]
    fn test_sync_result_large_number_of_wallets() {
        let mut result = SyncResult::new();

        // Add 100 wallets
        for i in 0..100u8 {
            result.added.push(test_address(i));
        }

        // 50 unchanged
        for i in 100..150u8 {
            result.unchanged.push(test_address(i));
        }

        // 10 errors
        for i in 0..10 {
            result.errors.push(format!("Error {i}"));
        }

        assert_eq!(result.added.len(), 100);
        assert_eq!(result.unchanged.len(), 50);
        assert_eq!(result.errors.len(), 10);
        assert_eq!(result.total_successful(), 150);
        assert_eq!(result.total_processed(), 160);

        // Success rate should be 150/160 = 93.75%
        assert_eq!(result.success_rate(), 93.75);
    }

    // ========================================
    // Error message format tests
    // ========================================

    #[test]
    fn test_sync_result_error_message_format() {
        let mut result = SyncResult::new();
        let address = test_address(0xAB);
        let error_msg = format!("Wallet {address}: Connection timeout");
        result.errors.push(error_msg.clone());

        // Address uses checksummed format: starts with 0x and contains hex characters
        assert!(result.errors[0].starts_with("Wallet 0x"));
        assert!(result.errors[0].contains("Connection timeout"));
    }

    #[test]
    fn test_sync_result_multiple_error_types() {
        let mut result = SyncResult::new();
        result
            .errors
            .push("Wallet 0x01: Redis connection failed".to_string());
        result
            .errors
            .push("Wallet 0x02: Invalid wallet format".to_string());
        result
            .errors
            .push("Wallet 0x03: Network timeout".to_string());

        assert_eq!(result.errors.len(), 3);
        assert!(result.errors[0].contains("Redis"));
        assert!(result.errors[1].contains("Invalid"));
        assert!(result.errors[2].contains("timeout"));
    }
}
