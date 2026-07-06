// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

/// @title MockMulticall3
/// @notice Minimal Multicall3 stand-in for integration tests
/// @dev Matches the real Multicall3 semantics for the subset the-beaconator
///      uses: aggregate3 (beacon batch updates, touch dispatch, wallet
///      balance sweep) executes each Call3 and only lets a call revert the
///      batch when allowFailure is false; getEthBalance backs the balance
///      sweep.
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

    function getEthBalance(address addr) external view returns (uint256 balance) {
        return addr.balance;
    }
}
