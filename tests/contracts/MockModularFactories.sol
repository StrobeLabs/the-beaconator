// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

// ---------------------------------------------------------------------------
// MockComponent - minimal contract deployed by all factories
// ---------------------------------------------------------------------------

contract MockComponent {
    address public factory;

    constructor() {
        factory = msg.sender;
    }
}

// ---------------------------------------------------------------------------
// Verifier
// ---------------------------------------------------------------------------

contract MockEcdsaVerifierFactory {
    event VerifierCreated(address verifier);

    function createVerifier(address signer) external returns (address) {
        MockComponent c = new MockComponent();
        emit VerifierCreated(address(c));
        return address(c);
    }
}

// ---------------------------------------------------------------------------
// Preprocessors
// ---------------------------------------------------------------------------

contract MockIdentityPreprocessorFactory {
    event PreprocessorCreated(address preprocessor);

    function createPreprocessor(uint256 measurementScale) external returns (address) {
        MockComponent c = new MockComponent();
        emit PreprocessorCreated(address(c));
        return address(c);
    }
}

contract MockThresholdFactory {
    event PreprocessorCreated(address preprocessor);

    function createPreprocessor(uint256 measurementScale, uint256 threshold) external returns (address) {
        MockComponent c = new MockComponent();
        emit PreprocessorCreated(address(c));
        return address(c);
    }
}

contract MockTernaryToBinaryFactory {
    event PreprocessorCreated(address preprocessor);

    function createPreprocessor(uint256 measurementScale, uint256 threshold) external returns (address) {
        MockComponent c = new MockComponent();
        emit PreprocessorCreated(address(c));
        return address(c);
    }
}

contract MockArgmaxFactory {
    event PreprocessorCreated(address preprocessor);

    function createPreprocessor(uint256 measurementScale) external returns (address) {
        MockComponent c = new MockComponent();
        emit PreprocessorCreated(address(c));
        return address(c);
    }
}

// ---------------------------------------------------------------------------
// BaseFns
// ---------------------------------------------------------------------------

contract MockCGBMFactory {
    event BaseFnCreated(address baseFn);

    function createBaseFn(
        uint256 sigmaBase,
        uint256 scalingFactor,
        uint256 alpha,
        uint256 decay,
        uint256 initialSigmaRatio,
        bool varianceScaling
    ) external returns (address) {
        MockComponent c = new MockComponent();
        emit BaseFnCreated(address(c));
        return address(c);
    }
}

contract MockDGBMFactory {
    event BaseFnCreated(address baseFn);

    function createBaseFn(
        uint256 sigmaBase,
        uint256 scalingFactor,
        uint256 decay,
        uint256 initialPositiveRate
    ) external returns (address) {
        MockComponent c = new MockComponent();
        emit BaseFnCreated(address(c));
        return address(c);
    }
}

// ---------------------------------------------------------------------------
// Transforms
// ---------------------------------------------------------------------------

contract MockBoundedFactory {
    event TransformCreated(address transform);

    function createTransform(
        uint256 minIndex,
        uint256 maxIndex,
        uint256 steepness
    ) external returns (address) {
        MockComponent c = new MockComponent();
        emit TransformCreated(address(c));
        return address(c);
    }
}

contract MockUnboundedFactory {
    event TransformCreated(address transform);

    function createTransform(uint256 initialIndex) external returns (address) {
        MockComponent c = new MockComponent();
        emit TransformCreated(address(c));
        return address(c);
    }
}

// ---------------------------------------------------------------------------
// Composers
// ---------------------------------------------------------------------------

contract MockWeightedSumComponentFactory {
    event ComposerCreated(address composer);

    function createComposer(uint256[] memory weights) external returns (address) {
        MockComponent c = new MockComponent();
        emit ComposerCreated(address(c));
        return address(c);
    }
}

// ---------------------------------------------------------------------------
// GroupFns
// ---------------------------------------------------------------------------

contract MockDominanceFactory {
    event GroupFnCreated(address groupFn);

    function createGroupFn(
        uint256 numClasses,
        uint256 alpha,
        uint256 decay,
        uint256[] memory initialEma
    ) external returns (address) {
        MockComponent c = new MockComponent();
        emit GroupFnCreated(address(c));
        return address(c);
    }
}

contract MockRelativeDominanceFactory {
    event GroupFnCreated(address groupFn);

    function createGroupFn(
        uint256 numClasses,
        uint256 alpha,
        uint256 decayFast,
        uint256 decaySlow,
        uint256[] memory initialMFast,
        uint256[] memory initialMSlow
    ) external returns (address) {
        MockComponent c = new MockComponent();
        emit GroupFnCreated(address(c));
        return address(c);
    }
}

contract MockContinuousAllocationFactory {
    event GroupFnCreated(address groupFn);

    function createGroupFn(
        uint256[] memory classProbs,
        uint256 sigmaBase,
        uint256 scaleFactor,
        uint256 decay
    ) external returns (address) {
        MockComponent c = new MockComponent();
        emit GroupFnCreated(address(c));
        return address(c);
    }
}

contract MockDiscreteAllocationFactory {
    event GroupFnCreated(address groupFn);

    function createGroupFn(
        uint256[] memory classProbs,
        uint256 sigmaBase,
        uint256 scaleFactor,
        uint256 decay
    ) external returns (address) {
        MockComponent c = new MockComponent();
        emit GroupFnCreated(address(c));
        return address(c);
    }
}

// ---------------------------------------------------------------------------
// GroupTransforms
// ---------------------------------------------------------------------------

contract MockSoftmaxFactory {
    event GroupTransformCreated(address groupTransform);

    function createGroupTransform(
        uint256 steepness,
        uint256 indexScale
    ) external returns (address) {
        MockComponent c = new MockComponent();
        emit GroupTransformCreated(address(c));
        return address(c);
    }
}

contract MockGMNormalizeFactory {
    event GroupTransformCreated(address groupTransform);

    function createGroupTransform(uint256 indexScale) external returns (address) {
        MockComponent c = new MockComponent();
        emit GroupTransformCreated(address(c));
        return address(c);
    }
}

// ---------------------------------------------------------------------------
// Beacon Factories
// ---------------------------------------------------------------------------

contract MockIdentityBeaconFactory {
    event BeaconCreated(address beacon);

    function createBeacon(
        address verifier,
        uint256 initialIndex
    ) external returns (address) {
        MockComponent c = new MockComponent();
        emit BeaconCreated(address(c));
        return address(c);
    }
}

contract MockStandaloneBeaconFactory {
    event BeaconCreated(address beacon);

    function createBeacon(
        address verifier,
        address preprocessor,
        address baseFn,
        address transform,
        uint256 initialIndex
    ) external returns (address) {
        MockComponent c = new MockComponent();
        emit BeaconCreated(address(c));
        return address(c);
    }
}

contract MockCompositeBeaconFactory {
    event BeaconCreated(address beacon);

    function createBeacon(
        address[] memory referenceBeacons,
        address composer
    ) external returns (address) {
        MockComponent c = new MockComponent();
        emit BeaconCreated(address(c));
        return address(c);
    }
}

contract MockGroupManagerFactory {
    event GroupManagerCreated(address groupManager);

    function createGroupManager(
        uint256[] memory initialIndices,
        int256[] memory initialZSpaceIndices,
        address verifier,
        address groupFn,
        address groupTransform
    ) external returns (address) {
        MockComponent c = new MockComponent();
        emit GroupManagerCreated(address(c));
        return address(c);
    }
}
