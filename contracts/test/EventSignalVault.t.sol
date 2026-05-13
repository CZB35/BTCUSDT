// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {EventSignalVault, IERC20} from "../src/EventSignalVault.sol";

interface Vm {
    function expectEmit(bool checkTopic1, bool checkTopic2, bool checkTopic3, bool checkData, address emitter)
        external;
    function expectRevert(bytes4 selector) external;
    function prank(address sender) external;
    function startPrank(address sender) external;
    function stopPrank() external;
}

contract MockToken is IERC20 {
    string public constant name = "Mock USDT";
    string public constant symbol = "USDT";
    uint8 public constant decimals = 18;

    mapping(address => uint256) public override balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        return true;
    }

    function transfer(address to, uint256 amount) external override returns (bool) {
        _transfer(msg.sender, to, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external override returns (bool) {
        uint256 approved = allowance[from][msg.sender];
        require(approved >= amount, "ALLOWANCE");
        if (approved != type(uint256).max) {
            allowance[from][msg.sender] = approved - amount;
        }
        _transfer(from, to, amount);
        return true;
    }

    function _transfer(address from, address to, uint256 amount) private {
        require(to != address(0), "TO_ZERO");
        require(balanceOf[from] >= amount, "BALANCE");
        balanceOf[from] -= amount;
        balanceOf[to] += amount;
    }
}

contract EventSignalVaultTest {
    Vm private constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    event DepositReceived(
        bytes32 indexed accountId,
        bytes32 indexed depositId,
        address indexed payer,
        address receiver,
        uint256 amount
    );
    event RefundPaid(
        bytes32 indexed refundId,
        address indexed payer,
        address indexed recipient,
        uint256 amount
    );

    MockToken private token;
    MockToken private secondToken;
    EventSignalVault private vault;

    address private constant ALICE = address(0xA11CE);
    address private constant BOB = address(0xB0B);
    address private constant SETTLEMENT = address(0x5E771E);
    address private constant REFUND_PAYER = address(0xFEE);
    address private constant NEW_OWNER = address(0x0A44);

    bytes32 private constant ACCOUNT_ID = keccak256("user-1");
    bytes32 private constant DEPOSIT_ID = keccak256("deposit-1");
    bytes32 private constant REFUND_ID = keccak256("refund-1");

    function setUp() public {
        token = new MockToken();
        secondToken = new MockToken();
        vault = new EventSignalVault(address(token), address(this), SETTLEMENT);
        token.mint(ALICE, 100 ether);
        token.mint(REFUND_PAYER, 50 ether);
        secondToken.mint(ALICE, 100 ether);
    }

    function testDepositForwardsTokensToSettlementWalletAndRecordsId() public {
        vm.startPrank(ALICE);
        token.approve(address(vault), 10 ether);

        vm.expectEmit(true, true, true, true, address(vault));
        emit DepositReceived(ACCOUNT_ID, DEPOSIT_ID, ALICE, SETTLEMENT, 10 ether);
        vault.deposit(ACCOUNT_ID, DEPOSIT_ID, 10 ether);
        vm.stopPrank();

        assertEq(token.balanceOf(address(vault)), 0, "vault balance");
        assertEq(token.balanceOf(SETTLEMENT), 10 ether, "settlement balance");
        assertEq(token.balanceOf(ALICE), 90 ether, "payer balance");
        assertTrue(vault.usedDepositIds(DEPOSIT_ID), "deposit id used");
    }

    function testDepositRejectsDuplicateDepositId() public {
        vm.startPrank(ALICE);
        token.approve(address(vault), 20 ether);
        vault.deposit(ACCOUNT_ID, DEPOSIT_ID, 10 ether);

        vm.expectRevert(EventSignalVault.DuplicateDepositId.selector);
        vault.deposit(ACCOUNT_ID, DEPOSIT_ID, 10 ether);
        vm.stopPrank();
    }

    function testDepositRejectsZeroAmount() public {
        vm.startPrank(ALICE);
        token.approve(address(vault), 1 ether);

        vm.expectRevert(EventSignalVault.InvalidAmount.selector);
        vault.deposit(ACCOUNT_ID, DEPOSIT_ID, 0);
        vm.stopPrank();
    }

    function testAnyFundedWalletCanPayRefundThroughContract() public {
        vm.startPrank(REFUND_PAYER);
        token.approve(address(vault), 18 ether);

        vm.expectEmit(true, true, true, true, address(vault));
        emit RefundPaid(REFUND_ID, REFUND_PAYER, BOB, 18 ether);
        vault.payRefund(BOB, 18 ether, REFUND_ID);
        vm.stopPrank();

        assertEq(token.balanceOf(BOB), 18 ether, "recipient balance");
        assertEq(token.balanceOf(REFUND_PAYER), 32 ether, "payer balance");
        assertEq(token.balanceOf(address(vault)), 0, "vault balance");
    }

    function testRefundRejectsZeroAmount() public {
        vm.startPrank(REFUND_PAYER);
        token.approve(address(vault), 1 ether);

        vm.expectRevert(EventSignalVault.InvalidAmount.selector);
        vault.payRefund(BOB, 0, REFUND_ID);
        vm.stopPrank();
    }

    function testOwnerCanUpdateSettlementWallet() public {
        address newSettlement = address(0x1234);
        vault.setSettlementWallet(newSettlement);
        assertEqAddress(vault.settlementWallet(), newSettlement, "settlement wallet");
    }

    function testOwnerCanUpdateAssetAddress() public {
        vault.setAsset(address(secondToken));
        assertEqAddress(address(vault.asset()), address(secondToken), "asset");

        vm.startPrank(ALICE);
        secondToken.approve(address(vault), 11 ether);
        vault.deposit(ACCOUNT_ID, DEPOSIT_ID, 11 ether);
        vm.stopPrank();

        assertEq(secondToken.balanceOf(SETTLEMENT), 11 ether, "settlement second token balance");
        assertEq(token.balanceOf(SETTLEMENT), 0, "settlement old token balance");
    }

    function testNonOwnerCannotUpdateSettlementWallet() public {
        vm.expectRevert(EventSignalVault.Unauthorized.selector);

        vm.prank(ALICE);
        vault.setSettlementWallet(BOB);
    }

    function testNonOwnerCannotUpdateAssetAddress() public {
        vm.expectRevert(EventSignalVault.Unauthorized.selector);

        vm.prank(ALICE);
        vault.setAsset(address(secondToken));
    }

    function testOwnershipCanTransferAndOldOwnerLosesAdminRights() public {
        vault.transferOwnership(NEW_OWNER);
        assertEqAddress(vault.owner(), NEW_OWNER, "owner");

        vm.expectRevert(EventSignalVault.Unauthorized.selector);
        vault.setSettlementWallet(BOB);

        vm.prank(NEW_OWNER);
        vault.setSettlementWallet(BOB);
        assertEqAddress(vault.settlementWallet(), BOB, "settlement wallet");
    }

    function testPauseBlocksDepositAndRefund() public {
        vault.pause();

        vm.startPrank(ALICE);
        token.approve(address(vault), 10 ether);
        vm.expectRevert(EventSignalVault.ContractPaused.selector);
        vault.deposit(ACCOUNT_ID, DEPOSIT_ID, 10 ether);
        vm.stopPrank();

        vm.startPrank(REFUND_PAYER);
        token.approve(address(vault), 10 ether);
        vm.expectRevert(EventSignalVault.ContractPaused.selector);
        vault.payRefund(BOB, 10 ether, REFUND_ID);
        vm.stopPrank();
    }

    function testOwnerCanRescueAccidentalTokenBalance() public {
        token.mint(address(vault), 3 ether);
        vault.rescueToken(address(token), SETTLEMENT, 3 ether);

        assertEq(token.balanceOf(SETTLEMENT), 3 ether, "rescued balance");
    }

    function assertEq(uint256 actual, uint256 expected, string memory message) private pure {
        if (actual != expected) {
            revert(message);
        }
    }

    function assertEqAddress(address actual, address expected, string memory message) private pure {
        if (actual != expected) {
            revert(message);
        }
    }

    function assertTrue(bool value, string memory message) private pure {
        if (!value) {
            revert(message);
        }
    }
}
