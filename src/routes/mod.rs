use alloy::sol;

pub mod beacon;
pub mod beacon_type;
pub mod info;
pub mod perp;
pub mod wallet;

#[cfg(test)]
// test_utils moved to tests/test_utils.rs
// Re-export all route functions for easy access
pub use beacon::*;
pub use beacon_type::*;
pub use info::*;
pub use perp::*;
pub use wallet::*;

// Define contract interfaces using Alloy's sol! macro - shared across all route modules
sol! {
    #[sol(rpc)]
    interface IBeacon {
        function index() external view returns (uint256);
        function update(bytes calldata proof, bytes calldata inputs) external;
        function twAvg(uint32 secondsAgo) external view returns (uint256);
        function increaseCardinalityCap(uint16 newCap) external;
        function verifier() external view returns (address);
        event IndexUpdated(uint256 index);
    }

    #[sol(rpc)]
    interface ICompositeBeacon {
        function index() external view returns (uint256);
        function update() external;
        function twAvg(uint32 secondsAgo) external view returns (uint256);
        function increaseCardinalityCap(uint16 newCap) external;
        event IndexUpdated(uint256 index);
    }

    #[sol(rpc)]
    interface IBeaconRegistry {
        function registerBeacon(address beacon) external;
        function unregisterBeacon(address beacon) external;
        function isBeaconRegistered(address beacon) external view returns (bool);
    }

    #[sol(rpc)]
    interface IEcdsaVerifier {
        function digest(uint256[] calldata measurement, uint256 nonce) external view returns (bytes32);
        function domainSeparator() external view returns (bytes32);
        function SIGNER() external view returns (address);
        function MEASUREMENT_TYPEHASH() external view returns (bytes32);
        function verify(bytes calldata proof, bytes calldata inputs) external returns (uint256[] memory);
        function usedProofs(bytes32 proofHash) external view returns (bool);
    }

    #[sol(rpc)]
    interface IEcdsaVerifierFactory {
        function createVerifier(address signer) external returns (address);
    }

    #[sol(rpc)]
    interface IIdentityFactory {
        function createBeacon(address signer, uint256 initialIndex) external returns (address);
    }

    #[sol(rpc)]
    interface IWeightedSumCompositeFactory {
        function createBeacon(address[] memory referenceBeacons, uint256[] memory weights) external returns (address);
    }

    #[sol(rpc)]
    interface IERC20 {
        function transfer(address to, uint256 amount) external returns (bool);
        function approve(address spender, uint256 amount) external returns (bool);
        function balanceOf(address account) external view returns (uint256 balance);
        function allowance(address owner, address spender) external view returns (uint256);
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
    interface IPerpManager {
        // Module interfaces for modular configuration
        type IFees is address;
        type IMarginRatios is address;
        type ILockupPeriod is address;
        type ISqrtPriceImpactLimit is address;

        // Modular CreatePerpParams struct - uses plugin modules instead of hardcoded values
        struct CreatePerpParams {
            address beacon;
            IFees fees;
            IMarginRatios marginRatios;
            ILockupPeriod lockupPeriod;
            ISqrtPriceImpactLimit sqrtPriceImpactLimit;
        }

        function createPerp(CreatePerpParams memory params) external returns (bytes32 perpId);
        event PerpCreated(bytes32 perpId, address beacon, uint256 sqrtPriceX96, uint256 indexPriceX96);

        struct OpenMakerPositionParams {
            address holder;
            uint128 margin;
            uint120 liquidity;
            int24 tickLower;
            int24 tickUpper;
            uint128 maxAmt0In;
            uint128 maxAmt1In;
        }

        function openMakerPos(bytes32 perpId, OpenMakerPositionParams memory params) external returns (uint256 posId);
        event PositionOpened(bytes32 perpId, uint256 sqrtPriceX96, uint256 longOI, uint256 shortOI, uint256 posId, bool isMaker, int256 perpDelta, int256 usdDelta, int24 tickLower, int24 tickUpper);
    }
}

// Separate module for LBCGBMFactory to allow clippy::too_many_arguments on generated code
#[allow(clippy::too_many_arguments, clippy::module_inception)]
mod lbcgbm_factory {
    alloy::sol! {
        #[sol(rpc)]
        interface ILBCGBMFactory {
            function createBeacon(
                address signer,
                uint256 measurementScale,
                uint256 sigmaBase,
                uint256 scalingFactor,
                uint256 alpha,
                uint256 decay,
                uint256 initialSigmaRatio,
                bool varianceScaling,
                uint256 minIndex,
                uint256 maxIndex,
                uint256 steepness,
                uint256 initialIndex
            ) external returns (address);
        }
    }
}
pub use lbcgbm_factory::ILBCGBMFactory;

// Re-export transaction utilities from services module
pub use crate::services::transaction::execution::is_nonce_error;
