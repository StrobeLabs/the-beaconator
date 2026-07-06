pub mod beacon;
pub mod beacon_type;
pub mod info;
pub mod perp;
pub mod recipe;
pub mod wallet;

#[cfg(test)]
// test_utils moved to tests/test_utils.rs
// Re-export all route functions for easy access
pub use beacon::*;
pub use beacon_type::*;
pub use info::*;
pub use perp::*;
pub use wallet::*;

// Define contract interfaces using Alloy's sol! macro - shared across all route modules.
// `#[allow(clippy::too_many_arguments)]` is needed for generated builder/call methods like
// PerpFactory.createPerp(owner, name, symbol, tokenUri, modules, emaWindow, salt) which
// expands to a Rust fn with 7+ args.
#[allow(clippy::too_many_arguments)]
mod root_sol_interfaces {
    use alloy::sol;

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

    // PerpFactory: deploys a per-market `Perp` contract for each beacon. v0.1.0 architecture
    // (perpcity-contracts@v0.1.0). Replaces the prior single-`PerpManager` design — see
    // `.contracts-versions` and CLAUDE.md.
    #[sol(rpc)]
    interface IPerpFactory {
        // Modules struct from src/libraries/SharedStructs.sol@v0.1.0. All addresses must already
        // be deployed module implementations (Fees, Funding, MarginRatios, PriceImpact, Pricing)
        // and a beacon registered with the BeaconRegistry.
        struct Modules {
            address beacon;
            address fees;
            address funding;
            address marginRatios;
            address priceImpact;
            address pricing;
        }

        function createPerp(
            address owner,
            string memory name,
            string memory symbol,
            string memory tokenUri,
            Modules memory modules,
            uint24 emaWindow,
            bytes32 salt
        ) external returns (address perp);

        // PerpFactory.perps mapping: tracks every address created by this factory. Used as the
        // membership check before any privileged action (USDC approval, openMaker call) on a
        // caller-supplied perp address — guarantees we never approve USDC to an EOA or a
        // contract that wasn't deployed by this trusted factory.
        function perps(address perp) external view returns (bool);

        event PerpCreated(
            address perp,
            bytes32 poolId,
            Modules modules,
            uint256 initialIndex,
            uint24 emaWindow,
            uint256 protocolFee,
            uint160 sqrtPriceX96,
            int24 tick,
            address owner,
            string name,
            string symbol,
            string tokenUri
        );

        error NotPoolManager();
        error StartingPriceTooLow();
        error StartingPriceTooHigh();
        error EmaWindowTooLow();
    }

    // Perp: per-market contract created by PerpFactory.createPerp. Each market has its own
    // Perp instance with its own ERC721 position NFTs and Uniswap V4 pool.
    #[sol(rpc)]
    interface IPerp {
        struct OpenMakerParams {
            address holder;
            uint128 margin;
            int24 tickLower;
            int24 tickUpper;
            uint128 liquidity;
            uint256 maxAmt0In;
            uint256 maxAmt1In;
        }

        struct OpenTakerParams {
            address holder;
            uint128 margin;
            int256 perpDelta;
            uint256 amt1Limit;
        }

        function openMaker(OpenMakerParams calldata params) external returns (uint256 posId);
        function openTaker(OpenTakerParams calldata params) external returns (uint256 posId);

        // Permissionless funding/EMA accrual (selector 0xa55526db). Called after a
        // beacon update to refresh funding for every perp backed by that beacon.
        function touch() external;

        event MakerOpened(uint256 posId);
        event TakerOpened(uint256 posId, SwapResult sr);

        // SwapResult is from src/libraries/SharedStructs.sol@v0.1.0.
        // BalanceDelta is a Uniswap V4 type aliased as int256 at the ABI level.
        struct SwapResult {
            int256 delta;
            uint256 ammPrice;
            int256 totalFeeAmt;
            uint256 lpFeeAmt;
            uint256 protocolFeeAmt;
            uint256 creatorFeeAmt;
            uint256 insuranceFeeAmt;
        }

        // Errors from src/libraries/Errors.sol@v0.1.0 reachable from openMaker / openTaker.
        // All parameterless — see ContractErrorDecoder in services/perp/validation.rs.
        error ZeroDelta();
        error MinAmtUnmet();
        error MarginTooLow();
        error ZeroLiquidity();
        error MaxAmtExceeded();
        error TicksOutOfBounds();
        error MarginRatioTooLow();
        error PriceImpactTooHigh();
        error UnauthorizedCaller();
        error PositionDoesNotExist();
        error LongUtilizationExceeded();
        error ShortUtilizationExceeded();
        error InsufficientLiquidityToFill();
        error Abdicated();
    }
    }
}
pub use root_sol_interfaces::{
    IBeacon, IBeaconRegistry, ICompositeBeacon, IERC20, IEcdsaVerifier, IEcdsaVerifierFactory,
    IIdentityFactory, IMulticall3, IPerp, IPerpFactory, IWeightedSumCompositeFactory,
};

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

// Component factories for modular beacon creation
#[allow(clippy::too_many_arguments, clippy::module_inception)]
mod component_factories {
    alloy::sol! {
        // ---- Beacon Factories ----
        #[sol(rpc)]
        interface IIdentityBeaconFactory {
            function createBeacon(address verifier, uint256 initialIndex) external returns (address);
        }

        #[sol(rpc)]
        interface IStandaloneBeaconFactory {
            function createBeacon(
                address verifier,
                address preprocessor,
                address baseFn,
                address transform,
                uint256 initialIndex
            ) external returns (address);
        }

        #[sol(rpc)]
        interface ICompositeBeaconFactory {
            function createBeacon(
                address[] memory referenceBeacons,
                address composer
            ) external returns (address);
        }

        #[sol(rpc)]
        interface IGroupManagerFactory {
            function createGroupManager(
                uint256[] memory initialIndices,
                int256[] memory initialZSpaceIndices,
                address verifier,
                address groupFn,
                address groupTransform
            ) external returns (address);
        }

        // ---- Preprocessor Factories ----
        #[sol(rpc)]
        interface IIdentityPreprocessorFactory {
            function createPreprocessor(uint256 measurementScale) external returns (address);
        }

        #[sol(rpc)]
        interface IThresholdFactory {
            function createPreprocessor(uint256 measurementScale, uint256 threshold) external returns (address);
        }

        #[sol(rpc)]
        interface ITernaryToBinaryFactory {
            function createPreprocessor(uint256 measurementScale, uint256 threshold) external returns (address);
        }

        #[sol(rpc)]
        interface IArgmaxFactory {
            function createPreprocessor(uint256 measurementScale) external returns (address);
        }

        // ---- BaseFn Factories ----
        #[sol(rpc)]
        interface ICGBMFactory {
            function createBaseFn(
                uint256 sigmaBase,
                uint256 scalingFactor,
                uint256 alpha,
                uint256 decay,
                uint256 initialSigmaRatio
            ) external returns (address);
        }

        #[sol(rpc)]
        interface IDGBMFactory {
            function createBaseFn(
                uint256 sigmaBase,
                uint256 scalingFactor,
                uint256 decay,
                uint256 initialPositiveRate
            ) external returns (address);
        }

        // ---- Transform Factories ----
        #[sol(rpc)]
        interface IBoundedFactory {
            function createTransform(uint256 minIndex, uint256 maxIndex, uint256 steepness) external returns (address);
        }

        #[sol(rpc)]
        interface IUnboundedFactory {
            function createTransform(uint256 initialIndex) external returns (address);
        }

        // ---- Composer Factories ----
        #[sol(rpc)]
        interface IWeightedSumComponentFactory {
            function createComposer(uint256[] memory weights) external returns (address);
        }

        // ---- GroupFn Factories ----
        #[sol(rpc)]
        interface IDominanceFactory {
            function createGroupFn(uint256 numClasses, uint256 alpha, uint256 decay, uint256[] memory initialEma) external returns (address);
        }

        #[sol(rpc)]
        interface IRelativeDominanceFactory {
            function createGroupFn(uint256 numClasses, uint256 alpha, uint256 decayFast, uint256 decaySlow, uint256[] memory initialMFast, uint256[] memory initialMSlow) external returns (address);
        }

        #[sol(rpc)]
        interface IContinuousAllocationFactory {
            function createGroupFn(uint256[] memory classProbs, uint256 sigmaBase, uint256 scaleFactor, uint256 decay) external returns (address);
        }

        #[sol(rpc)]
        interface IDiscreteAllocationFactory {
            function createGroupFn(uint256[] memory classProbs, uint256 sigmaBase, uint256 scaleFactor, uint256 decay) external returns (address);
        }

        // ---- GroupTransform Factories ----
        #[sol(rpc)]
        interface ISoftmaxFactory {
            function createGroupTransform(uint256 steepness, uint256 indexScale) external returns (address);
        }

        #[sol(rpc)]
        interface IGMNormalizeFactory {
            function createGroupTransform(uint256 indexScale) external returns (address);
        }
    }
}
pub use component_factories::*;

// Re-export transaction utilities from services module
pub use crate::services::transaction::execution::is_nonce_error;
