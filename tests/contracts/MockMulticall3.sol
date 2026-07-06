// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

/// Minimal Multicall3.aggregate3 stand-in for the touch integration test.
/// Matches the real Multicall3 semantics for the subset the-beaconator uses:
/// each Call3 is executed, and a call is only allowed to revert the batch when
/// allowFailure is false.
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

    function aggregate3(Call3[] calldata calls)
        external
        payable
        returns (Result[] memory returnData)
    {
        uint256 length = calls.length;
        returnData = new Result[](length);
        for (uint256 i = 0; i < length; i++) {
            (bool success, bytes memory ret) = calls[i].target.call(calls[i].callData);
            if (!calls[i].allowFailure) {
                require(success, "Multicall3: call failed");
            }
            returnData[i] = Result(success, ret);
        }
    }
}
