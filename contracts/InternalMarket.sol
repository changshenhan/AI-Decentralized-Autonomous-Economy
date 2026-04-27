// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {AccessControl} from "@openzeppelin/contracts/access/AccessControl.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {Treasury} from "./Treasury.sol";
import {IBankAudit} from "./interfaces/IBankAudit.sol";
import {IGuardrail} from "./interfaces/IGuardrail.sol";

/// @title InternalMarket
/// @notice 单币种 vSTK 内部账簿 + 订单事件（Rust Engine 撮合）；链上结算与 `Treasury.collectTax`；分红按份额累积，可路由至 `DualPoolVault`
contract InternalMarket is AccessControl, ReentrancyGuard {
    using SafeERC20 for IERC20;

    bytes32 public constant SETTLER_ROLE = keccak256("SETTLER_ROLE");
    bytes32 public constant COMPANY_ADMIN_ROLE = keccak256("COMPANY_ADMIN_ROLE");

    uint256 internal constant ONE = 1e18;

    IERC20 public immutable aitoken;
    Treasury public immutable treasury;
    IBankAudit public immutable bankAudit;

    /// @notice 可为 address(0) 表示未启用熔断
    IGuardrail public immutable guardrail;

    uint256 public nextCompanyId = 1;
    uint256 public nextOrderId = 1;

    /// @notice vSTK：公司 => 持有人 => 余额（LAW 第3条：非独立 ERC20）
    mapping(uint256 => mapping(address => uint256)) public vstkBalance;

    mapping(uint256 => uint256) public totalVstk;

    /// @notice 公司 => 关联 `DualPoolVault`（展示/路由说明）
    mapping(uint256 => address) public companyVault;

    mapping(uint256 => uint256) public accAitPerShare;
    mapping(uint256 => mapping(address => uint256)) public rewardDebt;

    mapping(uint256 => mapping(address => address)) public dividendPayoutVault;

    /// @notice 交易用 $AIT 内部余额（买单需先存入）
    mapping(address => uint256) public aitBalance;

    enum Side {
        Buy,
        Sell
    }

    struct Order {
        uint256 id;
        uint256 companyId;
        address maker;
        Side side;
        uint256 priceRay;
        uint256 remaining;
        bool active;
    }

    mapping(uint256 => Order) public orders;

    event CompanyRegistered(uint256 indexed companyId, address indexed vault, address indexed admin);
    event SharesIssued(uint256 indexed companyId, address indexed to, uint256 amount, uint256 newTotal);
    event AitDeposited(address indexed user, uint256 amount);
    event AitWithdrawn(address indexed user, uint256 amount);
    event OrderPlaced(
        uint256 indexed orderId,
        uint256 indexed companyId,
        address indexed maker,
        Side side,
        uint256 priceRay,
        uint256 amount,
        uint256 timestamp,
        uint256 engineHint
    );
    event OrderCancelled(uint256 indexed orderId, address indexed maker);
    event TradeSettled(
        uint256 indexed companyId,
        uint256 indexed takerOrderId,
        uint256 indexed makerOrderId,
        address buyer,
        address seller,
        uint256 vstkAmount,
        uint256 aitNotional,
        uint256 fee,
        bytes32 settlementId
    );
    event TradeSettledBlocked(
        uint256 indexed makerOrderId,
        uint256 indexed takerOrderId,
        uint256 aitNotional,
        bytes32 settlementId
    );
    event ProfitDepositBlocked(uint256 indexed companyId, uint256 grossAmount);
    event ProfitDeposited(uint256 indexed companyId, address indexed from, uint256 gross, uint256 fee, uint256 rewardBase);
    event DividendClaimed(uint256 indexed companyId, address indexed holder, address indexed to, uint256 amount);
    event PayoutVaultSet(uint256 indexed companyId, address indexed holder, address vault);

    error ZeroAddress();
    error BadOrder();
    error InsufficientVstk();
    error InsufficientAitBalance();
    error NotTradable();

    constructor(IERC20 aitoken_, Treasury treasury_, IBankAudit bankAudit_, address admin, address guardrail_) {
        if (address(aitoken_) == address(0) || address(treasury_) == address(0) || address(bankAudit_) == address(0) || admin == address(0)) {
            revert ZeroAddress();
        }
        aitoken = aitoken_;
        treasury = treasury_;
        bankAudit = bankAudit_;
        guardrail = IGuardrail(guardrail_);
        _grantRole(DEFAULT_ADMIN_ROLE, admin);
        _grantRole(SETTLER_ROLE, admin);
        _grantRole(COMPANY_ADMIN_ROLE, admin);
        SafeERC20.forceApprove(IERC20(address(aitoken_)), address(treasury_), type(uint256).max);
    }

    function _requireTradable(uint256 companyId) private view {
        if (!bankAudit.isTradable(companyId)) revert NotTradable();
    }

    function registerCompany(address vault) external onlyRole(COMPANY_ADMIN_ROLE) returns (uint256 companyId) {
        companyId = nextCompanyId++;
        companyVault[companyId] = vault;
        emit CompanyRegistered(companyId, vault, msg.sender);
    }

    function issueShares(uint256 companyId, address to, uint256 amount) external onlyRole(COMPANY_ADMIN_ROLE) {
        if (to == address(0)) revert ZeroAddress();
        if (companyId == 0 || companyId >= nextCompanyId) revert BadOrder();
        uint256 _acc = accAitPerShare[companyId];
        vstkBalance[companyId][to] += amount;
        totalVstk[companyId] += amount;
        rewardDebt[companyId][to] += (amount * _acc) / ONE;
        emit SharesIssued(companyId, to, amount, totalVstk[companyId]);
    }

    function depositAit(uint256 amount) external nonReentrant {
        aitoken.safeTransferFrom(msg.sender, address(this), amount);
        aitBalance[msg.sender] += amount;
        emit AitDeposited(msg.sender, amount);
    }

    function withdrawAit(uint256 amount) external nonReentrant {
        aitBalance[msg.sender] -= amount;
        aitoken.safeTransfer(msg.sender, amount);
        emit AitWithdrawn(msg.sender, amount);
    }

    /// @notice 下單：撮合在链下；买单请先 `depositAit`
    function placeOrder(
        uint256 companyId,
        Side side,
        uint256 priceRay,
        uint256 amount,
        uint256 engineHint
    ) external returns (uint256 orderId) {
        if (companyId == 0 || companyId >= nextCompanyId) revert BadOrder();
        _requireTradable(companyId);
        if (amount == 0 || priceRay == 0) revert BadOrder();
        if (side == Side.Sell) {
            if (vstkBalance[companyId][msg.sender] < amount) revert InsufficientVstk();
        }
        orderId = nextOrderId++;
        orders[orderId] = Order({
            id: orderId,
            companyId: companyId,
            maker: msg.sender,
            side: side,
            priceRay: priceRay,
            remaining: amount,
            active: true
        });
        emit OrderPlaced(orderId, companyId, msg.sender, side, priceRay, amount, block.timestamp, engineHint);
    }

    function cancelOrder(uint256 orderId) external {
        Order storage o = orders[orderId];
        if (!o.active) revert BadOrder();
        if (o.maker != msg.sender) revert BadOrder();
        o.active = false;
        emit OrderCancelled(orderId, msg.sender);
    }

    /// @notice Rust Engine 回传撮合结果：卖方出 vSTK，买方扣 `aitBalance` 并支付 AIT（经 `collectTax`）
    function settleTrade(
        uint256 makerOrderId,
        uint256 takerOrderId,
        uint256 vstkAmount,
        uint256 aitNotional,
        bytes32 settlementId
    ) external onlyRole(SETTLER_ROLE) nonReentrant {
        Order storage mo = orders[makerOrderId];
        Order storage to = orders[takerOrderId];
        if (!mo.active || !to.active) revert BadOrder();
        if (mo.companyId != to.companyId) revert BadOrder();
        if (mo.side == to.side) revert BadOrder();
        uint256 cid = mo.companyId;
        _requireTradable(cid);
        if (vstkAmount == 0 || aitNotional == 0) revert BadOrder();
        if (mo.remaining < vstkAmount || to.remaining < vstkAmount) revert BadOrder();

        uint256 expectedFee = (aitNotional * treasury.taxRateBps()) / 10_000;
        if (address(guardrail) != address(0)) {
            guardrail.requireNotPaused();
            if (!guardrail.evaluateSettleFee(expectedFee, aitNotional)) {
                emit TradeSettledBlocked(makerOrderId, takerOrderId, aitNotional, settlementId);
                return;
            }
        }

        mo.remaining -= vstkAmount;
        to.remaining -= vstkAmount;
        if (mo.remaining == 0) mo.active = false;
        if (to.remaining == 0) to.active = false;

        address seller = mo.side == Side.Sell ? mo.maker : to.maker;
        address buyer = mo.side == Side.Buy ? mo.maker : to.maker;

        if (aitBalance[buyer] < aitNotional) revert InsufficientAitBalance();
        aitBalance[buyer] -= aitNotional;

        uint256 fee = treasury.collectTax(address(this), aitNotional);
        uint256 net = aitNotional - fee;

        vstkBalance[cid][seller] -= vstkAmount;
        vstkBalance[cid][buyer] += vstkAmount;

        aitoken.safeTransfer(seller, net);

        emit TradeSettled(cid, takerOrderId, makerOrderId, buyer, seller, vstkAmount, aitNotional, fee, settlementId);
    }

    /// @notice 利润进入分红池：先对 `grossAmount` 抽税，再按份额累积（股东后续 `claimDividend`，可指向 `DualPoolVault`）
    function depositProfit(uint256 companyId, uint256 grossAmount) external nonReentrant {
        if (companyId == 0 || companyId >= nextCompanyId) revert BadOrder();
        if (grossAmount == 0) revert BadOrder();
        uint256 expectedFee = (grossAmount * treasury.taxRateBps()) / 10_000;
        if (address(guardrail) != address(0)) {
            guardrail.requireNotPaused();
            if (!guardrail.evaluateSettleFee(expectedFee, grossAmount)) {
                emit ProfitDepositBlocked(companyId, grossAmount);
                return;
            }
        }
        uint256 fee = treasury.collectTax(msg.sender, grossAmount);
        uint256 net = grossAmount - fee;
        aitoken.safeTransferFrom(msg.sender, address(this), net);
        _notifyReward(companyId, net);
        emit ProfitDeposited(companyId, msg.sender, grossAmount, fee, net);
    }

    function _notifyReward(uint256 companyId, uint256 reward) internal {
        uint256 supply = totalVstk[companyId];
        if (supply == 0) return;
        accAitPerShare[companyId] += (reward * ONE) / supply;
    }

    function setDividendPayoutVault(uint256 companyId, address vault) external {
        if (vstkBalance[companyId][msg.sender] == 0) revert BadOrder();
        dividendPayoutVault[companyId][msg.sender] = vault;
        emit PayoutVaultSet(companyId, msg.sender, vault);
    }

    function pendingDividend(uint256 companyId, address holder) public view returns (uint256) {
        uint256 b = vstkBalance[companyId][holder];
        uint256 _acc = accAitPerShare[companyId];
        uint256 debt = rewardDebt[companyId][holder];
        return (b * _acc) / ONE - debt;
    }

    function claimDividend(uint256 companyId) external nonReentrant {
        uint256 b = vstkBalance[companyId][msg.sender];
        uint256 _acc = accAitPerShare[companyId];
        uint256 debt = rewardDebt[companyId][msg.sender];
        uint256 pending = (b * _acc) / ONE - debt;
        if (pending == 0) return;
        rewardDebt[companyId][msg.sender] = (b * _acc) / ONE;
        address to = dividendPayoutVault[companyId][msg.sender];
        if (to == address(0)) to = msg.sender;
        aitoken.safeTransfer(to, pending);
        emit DividendClaimed(companyId, msg.sender, to, pending);
    }
}
