use alloy::primitives::{Address, B256, U256, keccak256};
use alloy::signers::Signer;
use alloy::signers::local::PrivateKeySigner;
use serde::{Deserialize, Serialize};

/// Client for the Safe Transaction Service API.
///
/// Proposes multisig transactions to a Gnosis Safe via the off-chain
/// Transaction Service, so they appear in the Safe UI for signing.
pub struct SafeTransactionService {
    client: reqwest::Client,
    base_url: String,
}

#[derive(Deserialize)]
struct SafeInfoResponse {
    nonce: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProposeTransactionRequest {
    to: String,
    value: String,
    data: String,
    operation: u8,
    safe_tx_gas: String,
    base_gas: String,
    gas_price: String,
    gas_token: String,
    refund_receiver: String,
    nonce: u64,
    contract_transaction_hash: String,
    sender: String,
    signature: String,
    origin: String,
}

impl SafeTransactionService {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Get the next nonce for the Safe from the Transaction Service.
    pub async fn get_nonce(&self, safe_address: Address) -> Result<u64, String> {
        let url = format!("{}/api/v1/safes/{:#x}/", self.base_url, safe_address);

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch Safe info: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!(
                "Safe Transaction Service returned {status}: {body}"
            ));
        }

        let info: SafeInfoResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Safe info response: {e}"))?;

        Ok(info.nonce)
    }

    /// Compute the EIP-712 Safe transaction hash.
    ///
    /// This follows the Gnosis Safe EIP-712 signing scheme:
    /// - Domain: {chainId, verifyingContract: safeAddress}
    /// - SafeTx type with all gas/payment fields set to 0
    pub fn encode_safe_tx_hash(
        safe_address: Address,
        chain_id: u64,
        to: Address,
        data: &[u8],
        nonce: u64,
    ) -> B256 {
        // EIP-712 domain separator
        let domain_type_hash = keccak256("EIP712Domain(uint256 chainId,address verifyingContract)");
        let domain_separator = keccak256(
            [
                domain_type_hash.as_slice(),
                &U256::from(chain_id).to_be_bytes::<32>(),
                &B256::left_padding_from(safe_address.as_slice()).0,
            ]
            .concat(),
        );

        // SafeTx type hash
        let safe_tx_type_hash = keccak256(
            "SafeTx(address to,uint256 value,bytes data,uint8 operation,uint256 safeTxGas,uint256 baseGas,uint256 gasPrice,address gasToken,address refundReceiver,uint256 nonce)",
        );

        // Encode the struct hash
        let data_hash = keccak256(data);
        let zero_u256 = U256::ZERO.to_be_bytes::<32>();
        let zero_address = B256::ZERO;

        let struct_hash = keccak256(
            [
                safe_tx_type_hash.as_slice(),              // typeHash
                &B256::left_padding_from(to.as_slice()).0, // to
                &zero_u256,                                // value = 0
                data_hash.as_slice(),                      // keccak256(data)
                &zero_u256,                                // operation = 0 (CALL)
                &zero_u256,                                // safeTxGas = 0
                &zero_u256,                                // baseGas = 0
                &zero_u256,                                // gasPrice = 0
                &zero_address.0,                           // gasToken = address(0)
                &zero_address.0,                           // refundReceiver = address(0)
                &U256::from(nonce).to_be_bytes::<32>(),    // nonce
            ]
            .concat(),
        );

        // EIP-712 final hash: keccak256("\x19\x01" || domainSeparator || structHash)
        keccak256(
            [
                &[0x19u8, 0x01u8] as &[u8],
                domain_separator.as_slice(),
                struct_hash.as_slice(),
            ]
            .concat(),
        )
    }

    /// Propose a transaction to the Safe Transaction Service.
    ///
    /// Signs the transaction with the provided signer and submits it.
    /// The signer must be one of the Safe owners.
    pub async fn propose_transaction(
        &self,
        safe_address: Address,
        chain_id: u64,
        to: Address,
        data: &[u8],
        nonce: u64,
        signer: &PrivateKeySigner,
    ) -> Result<B256, String> {
        let safe_tx_hash = Self::encode_safe_tx_hash(safe_address, chain_id, to, data, nonce);

        // Sign the hash
        let signature = signer
            .sign_hash(&safe_tx_hash)
            .await
            .map_err(|e| format!("Failed to sign Safe transaction: {e}"))?;

        // Encode signature as r + s + v (65 bytes)
        let mut sig_bytes = Vec::with_capacity(65);
        sig_bytes.extend_from_slice(&signature.r().to_be_bytes::<32>());
        sig_bytes.extend_from_slice(&signature.s().to_be_bytes::<32>());
        sig_bytes.push(if signature.v() { 28 } else { 27 });

        let sender = signer.address();

        let request_body = ProposeTransactionRequest {
            to: format!("{:#x}", to),
            value: "0".to_string(),
            data: format!("0x{}", hex::encode(data)),
            operation: 0,
            safe_tx_gas: "0".to_string(),
            base_gas: "0".to_string(),
            gas_price: "0".to_string(),
            gas_token: format!("{:#x}", Address::ZERO),
            refund_receiver: format!("{:#x}", Address::ZERO),
            nonce,
            contract_transaction_hash: format!("{:#x}", safe_tx_hash),
            sender: format!("{:#x}", sender),
            signature: format!("0x{}", hex::encode(&sig_bytes)),
            origin: "the-beaconator".to_string(),
        };

        let url = format!(
            "{}/api/v1/safes/{:#x}/multisig-transactions/",
            self.base_url, safe_address
        );

        let resp = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("Failed to propose Safe transaction: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!(
                "Safe Transaction Service returned {status}: {body}"
            ));
        }

        Ok(safe_tx_hash)
    }

    /// Returns the default Safe Transaction Service URL for a given chain ID.
    pub fn default_url_for_chain(chain_id: u64) -> Option<String> {
        match chain_id {
            84532 => Some("https://safe-transaction-base-sepolia.safe.global".to_string()),
            8453 => Some("https://safe-transaction-base.safe.global".to_string()),
            421614 => Some("https://safe-transaction-arbitrum-sepolia.safe.global".to_string()),
            42161 => Some("https://safe-transaction-arbitrum.safe.global".to_string()),
            _ => None,
        }
    }
}
