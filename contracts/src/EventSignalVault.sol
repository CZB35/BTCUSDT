// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

interface IERC20 {
    function balanceOf(address account) external view returns (uint256);
    function transfer(address to, uint256 value) external returns (bool);
    function transferFrom(address from, address to, uint256 value) external returns (bool);
}

contract EventSignalVault {
    IERC20 public asset;

    address public owner;
    address public settlementWallet;
    bool public paused;

    mapping(bytes32 => bool) public usedDepositIds;

    uint256 private reentrancyLock = 1;

    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event AssetUpdated(address indexed previousAsset, address indexed newAsset);
    event SettlementWalletUpdated(address indexed previousWallet, address indexed newWallet);
    event Paused(address indexed account);
    event Unpaused(address indexed account);
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
    event TokenRescued(address indexed token, address indexed to, uint256 amount);

    error ZeroAddress();
    error ZeroId();
    error InvalidAmount();
    error DuplicateDepositId();
    error Unauthorized();
    error ContractPaused();
    error Reentrancy();
    error TokenTransferFailed();

    constructor(address asset_, address owner_, address settlementWallet_) {
        if (asset_ == address(0) || owner_ == address(0) || settlementWallet_ == address(0)) {
            revert ZeroAddress();
        }

        asset = IERC20(asset_);
        owner = owner_;
        settlementWallet = settlementWallet_;

        emit OwnershipTransferred(address(0), owner_);
        emit AssetUpdated(address(0), asset_);
        emit SettlementWalletUpdated(address(0), settlementWallet_);
    }

    modifier onlyOwner() {
        if (msg.sender != owner) {
            revert Unauthorized();
        }
        _;
    }

    modifier whenNotPaused() {
        if (paused) {
            revert ContractPaused();
        }
        _;
    }

    modifier nonReentrant() {
        if (reentrancyLock != 1) {
            revert Reentrancy();
        }
        reentrancyLock = 2;
        _;
        reentrancyLock = 1;
    }

    function transferOwnership(address newOwner) external onlyOwner {
        if (newOwner == address(0)) {
            revert ZeroAddress();
        }

        address previousOwner = owner;
        owner = newOwner;

        emit OwnershipTransferred(previousOwner, newOwner);
    }

    function setAsset(address newAsset) external onlyOwner {
        if (newAsset == address(0)) {
            revert ZeroAddress();
        }

        address previousAsset = address(asset);
        asset = IERC20(newAsset);

        emit AssetUpdated(previousAsset, newAsset);
    }

    function setSettlementWallet(address newSettlementWallet) external onlyOwner {
        if (newSettlementWallet == address(0)) {
            revert ZeroAddress();
        }

        address previousWallet = settlementWallet;
        settlementWallet = newSettlementWallet;

        emit SettlementWalletUpdated(previousWallet, newSettlementWallet);
    }

    function pause() external onlyOwner {
        paused = true;
        emit Paused(msg.sender);
    }

    function unpause() external onlyOwner {
        paused = false;
        emit Unpaused(msg.sender);
    }

    function deposit(bytes32 accountId, bytes32 depositId, uint256 amount)
        external
        nonReentrant
        whenNotPaused
    {
        if (accountId == bytes32(0) || depositId == bytes32(0)) {
            revert ZeroId();
        }
        if (amount == 0) {
            revert InvalidAmount();
        }
        if (usedDepositIds[depositId]) {
            revert DuplicateDepositId();
        }

        usedDepositIds[depositId] = true;

        uint256 balanceBefore = asset.balanceOf(settlementWallet);
        _safeTransferFrom(address(asset), msg.sender, settlementWallet, amount);
        uint256 received = asset.balanceOf(settlementWallet) - balanceBefore;

        if (received != amount) {
            revert TokenTransferFailed();
        }

        emit DepositReceived(accountId, depositId, msg.sender, settlementWallet, amount);
    }

    function payRefund(address recipient, uint256 amount, bytes32 refundId)
        external
        nonReentrant
        whenNotPaused
    {
        if (recipient == address(0)) {
            revert ZeroAddress();
        }
        if (refundId == bytes32(0)) {
            revert ZeroId();
        }
        if (amount == 0) {
            revert InvalidAmount();
        }

        uint256 balanceBefore = asset.balanceOf(recipient);
        _safeTransferFrom(address(asset), msg.sender, recipient, amount);
        uint256 received = asset.balanceOf(recipient) - balanceBefore;

        if (received != amount) {
            revert TokenTransferFailed();
        }

        emit RefundPaid(refundId, msg.sender, recipient, amount);
    }

    function rescueToken(address token, address to, uint256 amount) external onlyOwner nonReentrant {
        if (token == address(0) || to == address(0)) {
            revert ZeroAddress();
        }
        if (amount == 0) {
            revert InvalidAmount();
        }

        _safeTransfer(token, to, amount);
        emit TokenRescued(token, to, amount);
    }

    function _safeTransfer(address token, address to, uint256 amount) private {
        (bool success, bytes memory data) =
            token.call(abi.encodeWithSelector(IERC20.transfer.selector, to, amount));

        if (!success || (data.length != 0 && !abi.decode(data, (bool)))) {
            revert TokenTransferFailed();
        }
    }

    function _safeTransferFrom(address token, address from, address to, uint256 amount) private {
        (bool success, bytes memory data) =
            token.call(abi.encodeWithSelector(IERC20.transferFrom.selector, from, to, amount));

        if (!success || (data.length != 0 && !abi.decode(data, (bool)))) {
            revert TokenTransferFailed();
        }
    }
}
