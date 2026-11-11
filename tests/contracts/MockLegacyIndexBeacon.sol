// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title MockLegacyIndexBeacon
/// @notice index() returns one word, like a beacons@v0.0.1 beacon.
contract MockLegacyIndexBeacon {
    uint256 public index = 1 << 96;
}
