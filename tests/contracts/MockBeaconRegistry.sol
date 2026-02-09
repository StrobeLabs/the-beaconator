// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title MockBeaconRegistry
/// @notice Minimal mock registry for integration testing
/// @dev Matches the real BeaconRegistry ABI interface
contract MockBeaconRegistry {
    // Event matches real ABI: BeaconRegistered(address beacon, uint256 data)
    event BeaconRegistered(address beacon, uint256 data);

    // Owner set to deployer for simplicity
    address public owner;

    // Named 'beacons' to match the expected interface (auto-generates beacons(address) getter)
    mapping(address => bool) public beacons;

    constructor() {
        owner = msg.sender;
    }

    /// @notice Register a beacon
    /// @param beacon The beacon address to register
    function registerBeacon(address beacon) external {
        beacons[beacon] = true;
        // Emit with data=0 for mock purposes
        emit BeaconRegistered(beacon, 0);
    }

    /// @notice Check if a beacon is registered (alternative getter)
    /// @param beacon The beacon address to check
    /// @return True if registered
    function isRegistered(address beacon) external view returns (bool) {
        return beacons[beacon];
    }
}
