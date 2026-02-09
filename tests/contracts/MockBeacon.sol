// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title MockBeacon
/// @notice Minimal mock beacon for integration testing
/// @dev Matches the real Beacon ABI interface
contract MockBeacon {
    // Event matches real ABI: DataUpdated(uint256 data)
    event DataUpdated(uint256 data);

    address public owner;
    bytes public lastProof;
    bytes public lastPublicSignals;

    constructor(address _owner) {
        owner = _owner;
    }

    /// @notice Update beacon data with proof
    /// @param proof The ZK proof data
    /// @param publicSignals The public signals
    function updateData(bytes calldata proof, bytes calldata publicSignals) external {
        lastProof = proof;
        lastPublicSignals = publicSignals;
        // Emit with a simple hash converted to uint256 for mock purposes
        emit DataUpdated(uint256(keccak256(abi.encodePacked(proof, publicSignals))));
    }

    /// @notice Get the owner address
    /// @return The owner address
    function getOwner() external view returns (address) {
        return owner;
    }
}
