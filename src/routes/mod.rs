use alloy::sol;

pub mod beacon;
pub mod info;
pub mod perp;
pub mod wallet;

#[cfg(test)]
mod test_utils;

// Re-export all route functions for easy access
pub use beacon::*;
pub use info::*;
pub use perp::*;
pub use wallet::*;

// Define contract interfaces using Alloy's sol! macro - shared across all route modules
sol! {
    #[sol(rpc)]
    interface IBeaconFactory {
        function createBeacon(address owner) external returns (address);
        event BeaconCreated(address beacon);
    }

    #[sol(rpc)]
    interface IBeaconRegistry {
        function registerBeacon(address beacon) external;
        function unregisterBeacon(address beacon) external;
        function beacons(address beacon) external view returns (bool);
    }

    #[sol(rpc)]
    interface IBeacon {
        function getData() external view returns (uint256 data, uint256 timestamp);
        function updateData(bytes calldata proof, bytes calldata publicSignals) external;
    }

    #[sol(rpc)]
    interface IDichotomousBeaconFactory {
        function createBeacon(address verifier, uint256 initialData, uint32 initialCardinalityNext) external returns (address);
        event BeaconCreated(address beacon, address verifier);
    }

    #[sol(rpc)]
    interface IStepBeacon {
        function getData() external view returns (uint256 data, uint256 timestamp);
        function updateData(bytes calldata proof, bytes calldata publicSignals) external;
        function getTwap(uint32 twapSecondsAgo) external view returns (uint256 twapPrice);
        function increaseCardinalityNext(uint32 cardinalityNext) external returns (uint32 cardinalityNextOld, uint32 cardinalityNextNew);
        event DataUpdated(uint256 data);
        error ProofAlreadyUsed(bytes proof, bytes publicSignals);
        error InvalidProof(bytes proof, bytes publicSignals);
    }

    #[sol(rpc)]
    interface IERC20 {
        function transfer(address to, uint256 amount) external returns (bool);
        function approve(address spender, uint256 amount) external returns (bool);
        function balanceOf(address account) external view returns (uint256 balance);
    }

    #[sol(rpc)]
    interface IMulticall3 {
        struct Call {
            address target;
            bytes callData;
        }

        struct Call3 {
            address target;
            bool allowFailure;
            bytes callData;
        }

        struct Result {
            bool success;
            bytes returnData;
        }

        function aggregate(Call[] calldata calls) external payable returns (uint256 blockNumber, bytes[] memory returnData);
        function aggregate3(Call3[] calldata calls) external payable returns (Result[] memory returnData);
        function tryAggregate(bool requireSuccess, Call[] calldata calls) external payable returns (Result[] memory returnData);
    }

    #[sol(rpc)]
    interface IPerpHook {
        // This struct matches the DEPLOYED PerpHook contract on Base Sepolia
        // Note: tradingFeeCreatorSplitX96 is NOT included in the deployed version
        struct CreatePerpParams {
            address beacon;
            uint24 tradingFee;
            uint128 minMargin;
            uint128 maxMargin;
            uint128 minOpeningLeverageX96;
            uint128 maxOpeningLeverageX96;
            uint128 liquidationLeverageX96;
            uint128 liquidationFeeX96;
            uint128 liquidationFeeSplitX96;
            int128 fundingInterval;
            int24 tickSpacing;
            uint160 startingSqrtPriceX96;
        }

        function createPerp(CreatePerpParams memory params) external returns (bytes32 perpId);
        event PerpCreated(bytes32 perpId, address beacon, uint256 markPriceX96);

        // Perp info struct - simplified version for checking if perp exists
        struct PerpInfo {
            address beacon;
            uint128 maxOpeningMargin;
            uint128 minOpeningMargin;
        }

        function perps(bytes32 perpId) external view returns (PerpInfo memory);

        struct OpenMakerPositionParams {
            uint128 margin;
            uint128 liquidity;
            int24 tickLower;
            int24 tickUpper;
        }

        struct MakerInfo {
            address holder;
            int24 tickLower;
            int24 tickUpper;
            uint160 sqrtPriceLowerX96;
            uint160 sqrtPriceUpperX96;
            uint128 margin;
            uint128 liquidity;
            uint128 perpsBorrowed;
            uint128 usdBorrowed;
            int128 entryTwPremiumX96;
            int128 entryTwPremiumDivBySqrtPriceX96;
        }

        function openMakerPosition(bytes32 perpId, OpenMakerPositionParams memory params) external returns (uint256 makerPosId);
        event MakerPositionOpened(bytes32 perpId, uint256 makerPosId, MakerInfo makerPos, uint256 markPriceX96);
    }
}

// Shared transaction serialization utilities
use crate::models::AppState;
use alloy::providers::Provider;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

// Global transaction mutex to serialize ALL blockchain transactions
// This prevents nonce conflicts by ensuring only one transaction is submitted at a time
static TRANSACTION_MUTEX: OnceLock<Arc<Mutex<()>>> = OnceLock::new();

fn get_transaction_mutex() -> &'static Arc<Mutex<()>> {
    TRANSACTION_MUTEX.get_or_init(|| Arc::new(Mutex::new(())))
}

// Helper function to get fresh nonce from alternate provider
pub async fn get_fresh_nonce_from_alternate(state: &AppState) -> Result<u64, String> {
    if let Some(alternate_provider) = &state.alternate_provider {
        tracing::info!("Getting fresh nonce from alternate RPC...");
        match alternate_provider
            .get_transaction_count(state.wallet_address)
            .await
        {
            Ok(nonce) => {
                tracing::info!("Fresh nonce from alternate RPC: {}", nonce);
                Ok(nonce)
            }
            Err(e) => {
                let error_msg = format!("Failed to get nonce from alternate RPC: {e}");
                tracing::error!("{}", error_msg);
                Err(error_msg)
            }
        }
    } else {
        Err("No alternate provider available".to_string())
    }
}

// Serialized transaction execution wrapper
// All blockchain transactions should use this to prevent nonce conflicts
// Alloy's wallet provider handles nonce management automatically
pub async fn execute_transaction_serialized<F, T>(operation: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let mutex = get_transaction_mutex();
    let _lock = mutex.lock().await;
    tracing::debug!("Acquired transaction lock - executing blockchain operation serially");
    let result = operation.await;
    tracing::debug!("Released transaction lock - blockchain operation completed");
    result
}

// Helper function to detect nonce-related errors
pub fn is_nonce_error(error_msg: &str) -> bool {
    let error_lower = error_msg.to_lowercase();
    error_lower.contains("nonce too low")
        || error_lower.contains("nonce too high")
        || error_lower.contains("invalid nonce")
        || error_lower.contains("replacement transaction underpriced")
}
