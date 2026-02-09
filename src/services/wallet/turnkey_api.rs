//! Turnkey API client extension for listing wallet accounts
//!
//! This module provides [`TurnkeyWalletAPI`] which wraps the Turnkey client
//! to provide wallet account listing functionality. It filters for Ethereum
//! (SECP256K1) wallets only.
//!
//! # Example
//!
//! ```rust,ignore
//! use the_beaconator::services::wallet::TurnkeyWalletAPI;
//!
//! let api = TurnkeyWalletAPI::new(
//!     "https://api.turnkey.com".to_string(),
//!     "org_xxx".to_string(),
//!     "api_public_key".to_string(),
//!     "api_private_key".to_string(),
//! )?;
//!
//! let accounts = api.list_wallet_accounts().await?;
//! for account in accounts {
//!     println!("Wallet: {} Address: {}", account.wallet_id, account.address);
//! }
//! ```

use alloy::primitives::Address;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use turnkey_client::generated::immutable::common::v1::Curve;
use turnkey_client::generated::services::coordinator::public::v1::GetWalletAccountsRequest;
use turnkey_client::{TurnkeyClient, TurnkeyClientError, TurnkeyP256ApiKey};

/// A wallet account retrieved from Turnkey, filtered for Ethereum compatibility.
#[derive(Debug, Clone)]
pub struct TurnkeyWalletAccount {
    /// The Turnkey wallet ID this account belongs to
    pub wallet_id: String,
    /// The Ethereum address of this account
    pub address: Address,
    /// The BIP-32 derivation path (e.g., "m/44'/60'/0'/0/0")
    pub path: String,
}

/// Error type for TurnkeyWalletAPI operations.
#[derive(Debug)]
pub enum TurnkeyWalletAPIError {
    /// Failed to create the Turnkey API key
    ApiKeyCreation(String),
    /// Failed to build the Turnkey client
    ClientBuild(String),
    /// Failed to make API request
    ApiRequest(TurnkeyClientError),
    /// Failed to parse an Ethereum address
    AddressParse { address: String, error: String },
}

impl fmt::Display for TurnkeyWalletAPIError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ApiKeyCreation(msg) => write!(f, "Failed to create Turnkey API key: {msg}"),
            Self::ClientBuild(msg) => write!(f, "Failed to build Turnkey client: {msg}"),
            Self::ApiRequest(e) => write!(f, "Turnkey API request failed: {e}"),
            Self::AddressParse { address, error } => {
                write!(f, "Failed to parse Ethereum address '{address}': {error}")
            }
        }
    }
}

impl std::error::Error for TurnkeyWalletAPIError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ApiRequest(e) => Some(e),
            _ => None,
        }
    }
}

impl From<TurnkeyClientError> for TurnkeyWalletAPIError {
    fn from(e: TurnkeyClientError) -> Self {
        Self::ApiRequest(e)
    }
}

/// Turnkey API client for listing wallet accounts.
///
/// This client wraps the Turnkey client and provides convenience methods
/// for retrieving wallet accounts, specifically filtered for Ethereum wallets
/// (SECP256K1 curve).
#[derive(Clone)]
pub struct TurnkeyWalletAPI {
    /// The underlying Turnkey client
    client: Arc<TurnkeyClient<TurnkeyP256ApiKey>>,
    /// The Turnkey organization ID
    organization_id: String,
}

impl TurnkeyWalletAPI {
    /// Create a new TurnkeyWalletAPI.
    ///
    /// # Arguments
    ///
    /// * `api_url` - Turnkey API base URL (e.g., "https://api.turnkey.com")
    /// * `organization_id` - Your Turnkey organization ID
    /// * `api_public_key` - API key public component (hex-encoded)
    /// * `api_private_key` - API key private component (hex-encoded)
    ///
    /// # Errors
    ///
    /// Returns an error if the API key cannot be parsed or the client cannot be built.
    pub fn new(
        api_url: String,
        organization_id: String,
        api_public_key: String,
        api_private_key: String,
    ) -> Result<Self, TurnkeyWalletAPIError> {
        // Create the API key from the provided credentials
        let api_key = TurnkeyP256ApiKey::from_strings(&api_private_key, Some(&api_public_key))
            .map_err(|e| TurnkeyWalletAPIError::ApiKeyCreation(e.to_string()))?;

        // Build the Turnkey client
        let client = TurnkeyClient::builder()
            .api_key(api_key)
            .base_url(&api_url)
            .build()
            .map_err(|e| TurnkeyWalletAPIError::ClientBuild(e.to_string()))?;

        Ok(Self {
            client: Arc::new(client),
            organization_id,
        })
    }

    /// List all Ethereum wallet accounts in the organization.
    ///
    /// This method calls Turnkey's `list_wallet_accounts` API and filters
    /// the results to only include accounts using the SECP256K1 curve
    /// (Ethereum-compatible wallets).
    ///
    /// # Returns
    ///
    /// A vector of [`TurnkeyWalletAccount`] containing wallet ID, Ethereum address,
    /// and derivation path for each Ethereum wallet account.
    ///
    /// # Errors
    ///
    /// Returns an error if the API request fails or if an address cannot be parsed.
    pub async fn list_wallet_accounts(
        &self,
    ) -> Result<Vec<TurnkeyWalletAccount>, TurnkeyWalletAPIError> {
        tracing::debug!(
            organization_id = %self.organization_id,
            "Fetching wallet accounts from Turnkey"
        );

        // Create the request to list all wallet accounts
        let request = GetWalletAccountsRequest {
            organization_id: self.organization_id.clone(),
            wallet_id: None, // Get all wallets
            include_wallet_details: Some(false),
            pagination_options: None,
        };

        // Make the API call
        let response = self.client.get_wallet_accounts(request).await?;

        tracing::debug!(
            total_accounts = response.accounts.len(),
            "Received wallet accounts from Turnkey"
        );

        // Filter for SECP256K1 curve (Ethereum wallets) and convert to our type
        let mut accounts = Vec::new();

        for account in response.accounts {
            // Only include SECP256K1 curve accounts (Ethereum)
            if account.curve != Curve::Secp256k1 {
                tracing::trace!(
                    wallet_id = %account.wallet_id,
                    curve = ?account.curve,
                    "Skipping non-SECP256K1 wallet account"
                );
                continue;
            }

            // Parse the address using FromStr
            let address = Address::from_str(&account.address).map_err(|e| {
                TurnkeyWalletAPIError::AddressParse {
                    address: account.address.clone(),
                    error: e.to_string(),
                }
            })?;

            accounts.push(TurnkeyWalletAccount {
                wallet_id: account.wallet_id,
                address,
                path: account.path,
            });
        }

        tracing::info!(
            ethereum_accounts = accounts.len(),
            "Filtered to Ethereum wallet accounts"
        );

        Ok(accounts)
    }

    /// Get the organization ID this client is configured for.
    pub fn organization_id(&self) -> &str {
        &self.organization_id
    }
}

impl std::fmt::Debug for TurnkeyWalletAPI {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnkeyWalletAPI")
            .field("organization_id", &self.organization_id)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turnkey_wallet_account_debug() {
        let account = TurnkeyWalletAccount {
            wallet_id: "wallet-123".to_string(),
            address: Address::from_str("0x0000000000000000000000000000000000000001").unwrap(),
            path: "m/44'/60'/0'/0/0".to_string(),
        };

        let debug_str = format!("{account:?}");
        assert!(debug_str.contains("wallet-123"));
        assert!(debug_str.contains("0x0000000000000000000000000000000000000001"));
    }

    #[test]
    fn test_error_display() {
        let err = TurnkeyWalletAPIError::ApiKeyCreation("test error".to_string());
        assert!(err.to_string().contains("test error"));

        let err = TurnkeyWalletAPIError::ClientBuild("build error".to_string());
        assert!(err.to_string().contains("build error"));

        let err = TurnkeyWalletAPIError::AddressParse {
            address: "0xinvalid".to_string(),
            error: "invalid hex".to_string(),
        };
        assert!(err.to_string().contains("0xinvalid"));
        assert!(err.to_string().contains("invalid hex"));
    }
}
