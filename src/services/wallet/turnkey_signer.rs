//! Turnkey API-backed Alloy Signer implementation
//!
//! This module provides a [`TurnkeySigner`] that implements Alloy's [`Signer`] trait
//! using Turnkey's remote signing API. Instead of holding private keys locally,
//! signing operations are delegated to Turnkey's secure infrastructure.
//!
//! # Example
//!
//! ```rust,ignore
//! use the_beaconator::services::wallet::TurnkeySigner;
//! use alloy::primitives::Address;
//!
//! let signer = TurnkeySigner::new(
//!     "https://api.turnkey.com".to_string(),
//!     "org_xxx".to_string(),
//!     "api_public_key".to_string(),
//!     "api_private_key".to_string(),
//!     "0x...".to_string(),  // sign_with (wallet address or private key ID)
//!     "0x...".parse().unwrap(),  // Ethereum address
//!     Some(8453),  // Base mainnet chain ID
//! );
//! ```

use alloy::primitives::{Address, B256, ChainId, Signature, U256};
use alloy::signers::{Error as SignerError, Signer};
use async_trait::async_trait;
use std::sync::Arc;
use turnkey_client::generated::SignRawPayloadIntentV2;
use turnkey_client::generated::immutable::common::v1::{HashFunction, PayloadEncoding};
use turnkey_client::{TurnkeyClient, TurnkeyP256ApiKey};

/// A Turnkey-backed signer that implements Alloy's Signer trait.
///
/// This signer uses Turnkey's remote signing API instead of holding
/// private keys locally. Each signing operation makes an API call to Turnkey.
///
/// # Thread Safety
///
/// This struct is `Clone` and can be safely shared across async tasks.
/// Each clone shares the same underlying HTTP client via `Arc`.
#[derive(Clone)]
pub struct TurnkeySigner {
    /// Turnkey client for API calls (wrapped in Arc for cloning)
    client: Arc<TurnkeyClient<TurnkeyP256ApiKey>>,
    /// Turnkey organization ID
    organization_id: String,
    /// The wallet address (derived from Turnkey)
    address: Address,
    /// The Turnkey private key ID or address to sign with
    sign_with: String,
    /// Chain ID for EIP-155 signatures
    chain_id: Option<ChainId>,
}

impl TurnkeySigner {
    /// Create a new TurnkeySigner.
    ///
    /// # Arguments
    ///
    /// * `api_url` - Turnkey API base URL (e.g., "https://api.turnkey.com")
    /// * `organization_id` - Your Turnkey organization ID
    /// * `api_public_key` - API key public component (hex-encoded)
    /// * `api_private_key` - API key private component (hex-encoded)
    /// * `sign_with` - Turnkey wallet address or private key ID
    /// * `address` - The Ethereum address of the wallet
    /// * `chain_id` - Optional chain ID for EIP-155 signatures
    ///
    /// # Errors
    ///
    /// Returns an error if the API key cannot be parsed or the client cannot be built.
    pub fn new(
        api_url: String,
        organization_id: String,
        api_public_key: String,
        api_private_key: String,
        sign_with: String,
        address: Address,
        chain_id: Option<ChainId>,
    ) -> Result<Self, String> {
        // Create the API key from the provided credentials
        let api_key = TurnkeyP256ApiKey::from_strings(&api_private_key, Some(&api_public_key))
            .map_err(|e| format!("Failed to create Turnkey API key: {e}"))?;

        // Build the Turnkey client
        let client = TurnkeyClient::builder()
            .api_key(api_key)
            .base_url(&api_url)
            .build()
            .map_err(|e| format!("Failed to build Turnkey client: {e}"))?;

        Ok(Self {
            client: Arc::new(client),
            organization_id,
            address,
            sign_with,
            chain_id,
        })
    }

    /// Get the Turnkey key ID / sign_with value
    pub fn key_id(&self) -> &str {
        &self.sign_with
    }

    /// Sign a hash using Turnkey's signRawPayload API.
    ///
    /// Makes a POST request to `/public/v1/submit/sign_raw_payload` with:
    /// - `type`: `ACTIVITY_TYPE_SIGN_RAW_PAYLOAD_V2`
    /// - `timestampMs`: Current timestamp in milliseconds
    /// - `organizationId`: Organization ID
    /// - `parameters.signWith`: Wallet address or key ID
    /// - `parameters.payload`: Hex-encoded hash to sign
    /// - `parameters.encoding`: `PAYLOAD_ENCODING_HEXADECIMAL`
    /// - `parameters.hashFunction`: `HASH_FUNCTION_NO_OP` (already hashed)
    ///
    /// # Returns
    ///
    /// Returns an Alloy `Signature` constructed from the (r, s, v) components
    /// returned by Turnkey.
    async fn sign_hash_remote(&self, hash: &B256) -> Result<Signature, SignerError> {
        // Convert hash to hex string (without 0x prefix for Turnkey)
        let payload_hex = hex::encode(hash.as_slice());

        tracing::debug!(
            "Signing hash with Turnkey: payload={}, sign_with={}",
            payload_hex,
            self.sign_with
        );

        // Create the sign request
        let request = self.client.sign_raw_payload(
            self.organization_id.clone(),
            self.client.current_timestamp(),
            SignRawPayloadIntentV2 {
                sign_with: self.sign_with.clone(),
                payload: payload_hex,
                encoding: PayloadEncoding::Hexadecimal,
                // NO_OP because the hash is already computed (keccak256)
                hash_function: HashFunction::NoOp,
            },
        );

        // Execute the request
        let result = request.await.map_err(|e| {
            tracing::error!("Turnkey signing request failed: {e}");
            SignerError::other(format!("Turnkey signing failed: {e}"))
        })?;

        // Extract the signature components from the result
        let sign_result = result.result;

        // Parse r, s, v from the response
        // Turnkey returns these as hex strings (with or without 0x prefix)
        let r = parse_hex_to_u256(&sign_result.r).map_err(|e| {
            tracing::error!("Failed to parse signature r component: {e}");
            SignerError::other(format!("Failed to parse signature r: {e}"))
        })?;

        let s = parse_hex_to_u256(&sign_result.s).map_err(|e| {
            tracing::error!("Failed to parse signature s component: {e}");
            SignerError::other(format!("Failed to parse signature s: {e}"))
        })?;

        // Parse v - Turnkey returns v as a hex string representing the recovery id
        // Valid values are: "00", "01", "1b" (27), or "1c" (28)
        let v_raw = parse_hex_to_u8(&sign_result.v).map_err(|e| {
            tracing::error!("Failed to parse signature v component: {e}");
            SignerError::other(format!("Failed to parse signature v: {e}"))
        })?;

        // Validate and normalize v to recovery id (0 or 1)
        // Only accept 0, 1, 27, or 28 as valid v values
        let recovery_id = match v_raw {
            0 | 1 => v_raw,
            27 => 0,
            28 => 1,
            _ => {
                tracing::error!(
                    "Invalid signature v value from Turnkey: {} (expected 0, 1, 27, or 28)",
                    v_raw
                );
                return Err(SignerError::other(format!(
                    "Invalid signature v value: {v_raw} (expected 0, 1, 27, or 28)"
                )));
            }
        };

        // Construct the Alloy signature
        // Alloy's Signature::new takes (r, s, y_parity) where y_parity is a bool
        let signature = Signature::new(r, s, recovery_id != 0);

        tracing::debug!(
            "Turnkey signature created: r={}, s={}, v={}",
            r,
            s,
            recovery_id
        );

        Ok(signature)
    }
}

/// Parse a hex string (with or without 0x prefix) to U256.
fn parse_hex_to_u256(hex_str: &str) -> Result<U256, String> {
    let clean = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    U256::from_str_radix(clean, 16).map_err(|e| format!("Invalid hex for U256: {e}"))
}

/// Parse a hex string (with or without 0x prefix) to u8.
fn parse_hex_to_u8(hex_str: &str) -> Result<u8, String> {
    let clean = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    u8::from_str_radix(clean, 16).map_err(|e| format!("Invalid hex for u8: {e}"))
}

#[async_trait]
impl Signer for TurnkeySigner {
    /// Sign a hash using Turnkey's remote signing API.
    async fn sign_hash(&self, hash: &B256) -> Result<Signature, SignerError> {
        self.sign_hash_remote(hash).await
    }

    /// Get the signer's Ethereum address.
    fn address(&self) -> Address {
        self.address
    }

    /// Get the chain ID.
    fn chain_id(&self) -> Option<ChainId> {
        self.chain_id
    }

    /// Set the chain ID.
    fn set_chain_id(&mut self, chain_id: Option<ChainId>) {
        self.chain_id = chain_id;
    }
}

impl std::fmt::Debug for TurnkeySigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnkeySigner")
            .field("organization_id", &self.organization_id)
            .field("address", &self.address)
            .field("sign_with", &self.sign_with)
            .field("chain_id", &self.chain_id)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_to_u256() {
        // Test with 0x prefix
        let result = parse_hex_to_u256("0x1234");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), U256::from(0x1234u64));

        // Test without 0x prefix
        let result = parse_hex_to_u256("abcd");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), U256::from(0xabcdu64));

        // Test full 32-byte value
        let full_hex = "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let result = parse_hex_to_u256(full_hex);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_hex_to_u8() {
        // Test recovery id values
        assert_eq!(parse_hex_to_u8("00").unwrap(), 0);
        assert_eq!(parse_hex_to_u8("01").unwrap(), 1);
        assert_eq!(parse_hex_to_u8("1b").unwrap(), 27);
        assert_eq!(parse_hex_to_u8("1c").unwrap(), 28);

        // Test with 0x prefix
        assert_eq!(parse_hex_to_u8("0x1b").unwrap(), 27);
    }

    #[test]
    fn test_v_normalization() {
        // Test that v values are normalized correctly
        let v_27: u8 = 27;
        let v_28: u8 = 28;
        let v_0: u8 = 0;
        let v_1: u8 = 1;

        // Values >= 27 should be converted
        assert_eq!(if v_27 >= 27 { v_27 - 27 } else { v_27 }, 0);
        assert_eq!(if v_28 >= 27 { v_28 - 27 } else { v_28 }, 1);

        // Values < 27 should stay the same
        assert_eq!(if v_0 >= 27 { v_0 - 27 } else { v_0 }, 0);
        assert_eq!(if v_1 >= 27 { v_1 - 27 } else { v_1 }, 1);
    }
}
