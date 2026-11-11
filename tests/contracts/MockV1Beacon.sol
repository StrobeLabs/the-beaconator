// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import { MockV1Verifier } from "./MockV1Verifier.sol";

/// @title MockV1Beacon
/// @notice index() returns (index, updatedAt), like a beacons@f762da1 IdentityBeacon.
contract MockV1Beacon {
    event IndexUpdated(uint256 index);

    MockV1Verifier public immutable verifier;
    uint256 private _index;
    uint256 private _updatedAt;

    constructor(MockV1Verifier verifier_, uint256 initialIndex) {
        verifier = verifier_;
        _index = initialIndex;
        _updatedAt = block.timestamp;
    }

    function index() external view returns (uint256, uint256) {
        return (_index, _updatedAt);
    }

    function update(bytes calldata proof, bytes calldata inputs) external {
        uint256[] memory data = verifier.verify(proof, inputs);
        _index = data[0];
        _updatedAt = block.timestamp;
        emit IndexUpdated(data[0]);
    }
}
