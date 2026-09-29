// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract RetirementHarness {
    address public funding;
    int88 public rate = 1;
    uint256 public pending;
    uint256 public nonce;
    bool public registered = true;
    bool public knownPerp = true;
    bool public disabled;

    function configure(address module, int88 newRate, uint256 deadline) external {
        funding = module;
        rate = newRate;
        pending = deadline;
    }
    function setKnown(bool known) external { knownPerp = known; }
    function setDisabled(bool value) external { disabled = value; }
    function owner() external view returns (address) { return address(this); }
    function perps(address) external view returns (bool) { return knownPerp; }
    function modules() external view returns (address, address, address, address, address, address) {
        return (address(this), address(0), funding, address(0), address(0), address(0));
    }
    function rates() external view returns (int88, uint64, uint64, uint40) {
        return (rate, 1, 1, uint40(block.timestamp));
    }
    function executableAt(bytes calldata) external view returns (uint256) { return pending; }
    function abdicated(bytes4) external view returns (bool) { return disabled; }
    function isBeaconRegistered(address) external view returns (bool) { return registered; }
    function unregisterBeacon(address) external { registered = false; }
    function submit(bytes calldata) external view { require(!disabled); }
    function setFundingModule(address module) external { funding = module; }
    function touch() external { rate = 0; }
}
