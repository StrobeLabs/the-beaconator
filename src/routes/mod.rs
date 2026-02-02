use alloy::sol;

pub mod beacon;
pub mod info;
pub mod perp;
pub mod wallet;

#[cfg(test)]
// test_utils moved to tests/test_utils.rs
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
        event DataUpdated(uint256 data);
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
    interface IEcdsaBeacon {
        function updateIndex(bytes calldata proof, bytes calldata inputs) external;
        function verifierAdapter() external view returns (address);
        function index() external view returns (uint256);
        event IndexUpdated(uint256 index);
    }

    #[sol(rpc)]
    interface IEcdsaVerifierAdapter {
        function digest(uint256 measurement, uint256 nonce) external view returns (bytes32);
        function domainSeparator() external view returns (bytes32);
        function SIGNER() external view returns (address);
        function MEASUREMENT_TYPEHASH() external view returns (bytes32);
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
            uint160 startingSqrtPriceX96;
        }

        function createPerp(CreatePerpParams memory params) external returns (bytes32 perpId);
        event PerpCreated(bytes32 perpId, address beacon, uint256 sqrtPriceX96, uint256 indexPriceX96);

        // Perp info struct - simplified version for checking if perp exists
        struct PerpInfo {
            address beacon;
            IFees fees;
            IMarginRatios marginRatios;
            ILockupPeriod lockupPeriod;
            ISqrtPriceImpactLimit sqrtPriceImpactLimit;
        }

        function perps(bytes32 perpId) external view returns (PerpInfo memory);

        struct OpenMakerPositionParams {
            address holder;
            uint256 margin;
            uint128 liquidity;
            int24 tickLower;
            int24 tickUpper;
            uint128 maxAmt0In;
            uint128 maxAmt1In;
        }

        struct MakerInfo {
            address holder;
            int24 tickLower;
            int24 tickUpper;
            uint160 sqrtPriceLowerX96;
            uint160 sqrtPriceUpperX96;
            uint256 margin;
            uint128 liquidity;
            uint128 perpsBorrowed;
            uint128 usdBorrowed;
            int128 entryTwPremiumX96;
            int128 entryTwPremiumDivBySqrtPriceX96;
        }

        function openMakerPos(bytes32 perpId, OpenMakerPositionParams memory params) external returns (uint256 posId);
        event PositionOpened(bytes32 perpId, uint256 sqrtPriceX96, uint256 longOI, uint256 shortOI, uint256 posId, bool isMaker, int256 entryPerpDelta);
    }
}

// Re-export transaction utilities from services module
pub use crate::services::transaction::execution::{
    execute_transaction_serialized, get_fresh_nonce_from_alternate, is_nonce_error,
};
