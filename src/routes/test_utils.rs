use crate::models::AppState;
#[cfg(test)]
use alloy::{json_abi::JsonAbi, primitives::Address};
use std::str::FromStr;
use std::sync::Arc;

#[cfg(test)]
pub fn create_test_app_state() -> AppState {
    // Create mock provider with wallet for testing - this won't work in real tests but allows compilation
    let signer = alloy::signers::local::PrivateKeySigner::random();
    let wallet = alloy::network::EthereumWallet::from(signer);
    // Use modern Alloy provider builder pattern for tests
    let provider = alloy::providers::ProviderBuilder::new()
        .wallet(wallet)
        .connect_http("http://localhost:8545".parse().unwrap());

    AppState {
        provider: Arc::new(provider),
        wallet_address: Address::from_str("0x1111111111111111111111111111111111111111").unwrap(),
        beacon_abi: JsonAbi::new(),
        beacon_factory_abi: JsonAbi::new(),
        beacon_registry_abi: JsonAbi::new(),
        perp_hook_abi: JsonAbi::new(),
        beacon_factory_address: Address::from_str("0x1234567890123456789012345678901234567890")
            .unwrap(),
        perpcity_registry_address: Address::from_str("0x2345678901234567890123456789012345678901")
            .unwrap(),
        perp_hook_address: Address::from_str("0x3456789012345678901234567890123456789012").unwrap(),
        access_token: "test_token".to_string(),
    }
}
