// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {EventSignalVault} from "../src/EventSignalVault.sol";

interface Vm {
    function envBytes32(string calldata name) external returns (bytes32);
    function envAddress(string calldata name) external returns (address);
    function startBroadcast(uint256 privateKey) external;
    function stopBroadcast() external;
}

contract DeployEventSignalVault {
    Vm private constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function run() external returns (EventSignalVault vault) {
        uint256 deployerPrivateKey = uint256(vm.envBytes32("BSC_DEPLOYER_PRIVATE_KEY"));
        address asset = vm.envAddress("BSC_USDT_CONTRACT");
        address owner = vm.envAddress("BSC_VAULT_OWNER");
        address settlementWallet = vm.envAddress("BSC_SETTLEMENT_WALLET");

        vm.startBroadcast(deployerPrivateKey);
        vault = new EventSignalVault(asset, owner, settlementWallet);
        vm.stopBroadcast();
    }
}
