use ethers::{
    abi::Abi,
    providers::{Http, Provider},
    signers::LocalWallet,
};
use rocket::serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateBeaconRequest {
    pub beacon_address: String,
    pub value: i64,
    pub proof: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateBeaconRequest {
    // TODO: Implement beacon creation parameters
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterBeaconRequest {
    // TODO: Implement beacon registration parameters
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: String,
}

// Cached application state
pub struct AppState {
    pub wallet: LocalWallet,
    pub provider: Arc<Provider<Http>>,
    pub beacon_abi: Abi,
    pub access_token: String,
}
