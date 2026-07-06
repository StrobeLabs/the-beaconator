// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

/// A touch() that always reverts, used to prove that
/// aggregate3(allowFailure = true) isolates a bad perp without reverting the
/// whole batch (an uninitialized-pool / bad-market perp in production).
contract RevertingTouchable {
    function touch() external pure {
        revert("touch reverts");
    }
}
