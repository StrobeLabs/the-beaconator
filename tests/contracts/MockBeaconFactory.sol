// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import "./MockBeacon.sol";

/// @title MockBeaconFactory
/// @notice Minimal mock factory for integration testing
/// @dev Deploys MockBeacon contracts and emits BeaconCreated events
contract MockBeaconFactory {
    event BeaconCreated(address beacon);

    /// @notice Create a new beacon
    /// @param owner The owner address for the beacon
    /// @return beacon The address of the deployed beacon
    function createBeacon(address owner) external returns (address beacon) {
        // Deploy a new MockBeacon
        MockBeacon newBeacon = new MockBeacon(owner);
        beacon = address(newBeacon);
        emit BeaconCreated(beacon);
    }
}
