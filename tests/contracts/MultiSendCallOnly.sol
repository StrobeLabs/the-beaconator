// SPDX-License-Identifier: LGPL-3.0-only
pragma solidity ^0.8.24;

/// Safe v1.4.1 MultiSendCallOnly, copied for tests: runs packed calls, never delegatecalls.
contract MultiSendCallOnly {
    function multiSend(bytes memory transactions) public payable {
        assembly {
            let length := mload(transactions)
            let i := 0x20
            for {} lt(i, length) {} {
                let operation := shr(0xf8, mload(add(transactions, i)))
                let to := shr(0x60, mload(add(transactions, add(i, 0x01))))
                to := or(to, mul(iszero(to), address()))
                let value := mload(add(transactions, add(i, 0x15)))
                let dataLength := mload(add(transactions, add(i, 0x35)))
                let data := add(transactions, add(i, 0x55))
                let success := 0
                switch operation
                case 0 { success := call(gas(), to, value, data, dataLength, 0, 0) }
                case 1 { revert(0, 0) }
                if eq(success, 0) {
                    let errorLength := returndatasize()
                    returndatacopy(0, 0, errorLength)
                    revert(0, errorLength)
                }
                i := add(i, add(0x55, dataLength))
            }
        }
    }
}
