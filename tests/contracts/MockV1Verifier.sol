// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title MockV1Verifier
/// @notice Test copy of the beacons@f762da1 ECDSAVerifier rules that the beaconator depends
/// on: a binder that binds once, and nonce == lastNonce + 1. The digest is a simple hash
/// (the beaconator reads it from digest(), so EIP-712 details do not matter here).
contract MockV1Verifier {
    error UnauthorizedCaller();
    error AlreadyBound();
    error InvalidAuthorizedCaller();
    error UnauthorizedBinder();
    error InvalidSigner();
    error InvalidNonce();

    address public immutable SIGNER;
    address public immutable binder;
    address public authorizedCaller;
    uint256 public lastNonce;

    constructor(address signer, address binder_) {
        SIGNER = signer;
        binder = binder_;
    }

    function bind(address caller) external {
        if (msg.sender != binder) revert UnauthorizedBinder();
        if (caller == address(0)) revert InvalidAuthorizedCaller();
        if (authorizedCaller != address(0)) revert AlreadyBound();
        authorizedCaller = caller;
    }

    function digest(uint256[] memory measurement, uint256 nonce) public view returns (bytes32) {
        return keccak256(abi.encode(keccak256(abi.encodePacked(measurement)), nonce, block.chainid, address(this)));
    }

    function verify(bytes calldata proof, bytes calldata inputs) external returns (uint256[] memory measurement) {
        if (msg.sender != authorizedCaller) revert UnauthorizedCaller();
        uint256 nonce;
        (measurement, nonce) = abi.decode(inputs, (uint256[], uint256));
        if (proof.length != 65) revert InvalidSigner();
        bytes32 r = bytes32(proof[0:32]);
        bytes32 s = bytes32(proof[32:64]);
        uint8 v = uint8(proof[64]);
        if (ecrecover(digest(measurement, nonce), v, r, s) != SIGNER) revert InvalidSigner();
        if (nonce != lastNonce + 1) revert InvalidNonce();
        lastNonce = nonce;
    }
}
