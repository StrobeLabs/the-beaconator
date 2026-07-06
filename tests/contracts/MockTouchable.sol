// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

/// Stand-in for a Perp exposing touch(): bumps a counter and emits an event, so
/// a receipt log from this address signals a successful touch (mirrors how the
/// worker infers per-perp success). Selector of touch() is 0xa55526db.
contract MockTouchable {
    uint256 public touchCount;

    event Touched(uint256 count);

    function touch() external {
        touchCount++;
        emit Touched(touchCount);
    }
}
