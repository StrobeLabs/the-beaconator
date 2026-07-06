// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// @title MockMulticall3
/// @notice Minimal Multicall3 mock for integration testing
/// @dev Implements the subset of the canonical Multicall3 the beaconator
///      uses: aggregate3 (beacon batch updates, wallet balance sweep) and
///      getEthBalance (balance sweep). Semantics match the real contract.
contract MockMulticall3 {
    struct Call3 {
        address target;
        bool allowFailure;
        bytes callData;
    }

    struct Result {
        bool success;
        bytes returnData;
    }

    function aggregate3(Call3[] calldata calls) external payable returns (Result[] memory returnData) {
        returnData = new Result[](calls.length);
        for (uint256 i = 0; i < calls.length; i++) {
            (bool success, bytes memory ret) = calls[i].target.call(calls[i].callData);
            if (!success && !calls[i].allowFailure) {
                revert("Multicall3: call failed");
            }
            returnData[i] = Result(success, ret);
        }
    }

    function getEthBalance(address addr) external view returns (uint256 balance) {
        return addr.balance;
    }
}
