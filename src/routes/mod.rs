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
    interface IERC20 {
        function transfer(address to, uint256 amount) external returns (bool);
        function approve(address spender, uint256 amount) external returns (bool);
        function balanceOf(address account) external view returns (uint256 balance);
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
