// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {IAIIdentityRegistry} from "./interfaces/IAIIdentityRegistry.sol";
import {AiPoolManager, AiPoolVaultBase} from "./AiPoolManager.sol";

/// @title DualPoolVault
/// @notice 双池核心：AiPool 出款在 `transferAiPoolAIT` 内直调库校验（防继承篡改）；分红至人类 EOA 仅当累计收入超过融资门槛（LAW 第2条、开发者指令第1条）
contract DualPoolVault is AiPoolVaultBase, Ownable {
    using SafeERC20 for IERC20;

    IERC20 public immutable aitoken;
    /// @notice 人类主人 DividendPool 收款地址（须为 EOA，LAW：人类分红池）
    address public immutable humanDividendRecipient;

    uint256 public financingThreshold;
    uint256 public cumulativeRevenue;
    uint256 public cumulativeDividendDistributed;

    error DividendProfitGate();
    error HumanMustBeEOA();
    error AiPoolInsufficientBalance();

    event FinancingThresholdUpdated(uint256 newThreshold);
    event RevenueRecorded(uint256 delta, uint256 newCumulative);
    event DividendTransferred(address indexed human, uint256 amount, uint256 newCumulativeDistributed);

    constructor(
        IAIIdentityRegistry registry_,
        IERC20 aitoken_,
        address humanDividendRecipient_,
        uint256 financingThreshold_,
        address initialOwner
    ) AiPoolVaultBase(registry_) Ownable(initialOwner) {
        require(address(aitoken_) != address(0), "DualPool: zero token");
        require(humanDividendRecipient_ != address(0), "DualPool: zero human");
        if (humanDividendRecipient_.code.length != 0) revert HumanMustBeEOA();
        aitoken = aitoken_;
        humanDividendRecipient = humanDividendRecipient_;
        financingThreshold = financingThreshold_;
    }

    function setFinancingThreshold(uint256 newThreshold) external onlyOwner {
        financingThreshold = newThreshold;
        emit FinancingThresholdUpdated(newThreshold);
    }

    function _recordRevenue(uint256 delta) internal virtual {
        cumulativeRevenue += delta;
        emit RevenueRecorded(delta, cumulativeRevenue);
    }

    /// @notice 链上记账：累计收入（由所有者/结算模块调用；高频路径下的可用分红额见 `availableDividend`）
    function recordRevenue(uint256 delta) external onlyOwner {
        _recordRevenue(delta);
    }

    /// @notice 高频只读：可分红余额 = max(0, min(链上余额, surplus - 已分红))，其中 surplus = revenue > threshold ? revenue - threshold : 0
    function availableDividend() public view returns (uint256) {
        return
            _availableDividendAsm(
                cumulativeRevenue,
                financingThreshold,
                cumulativeDividendDistributed,
                aitoken.balanceOf(address(this))
            );
    }

    /// @dev Yul：盈余与已分红差额的核心算术，减少分支与冗余分配（view 热路径）
    function _availableDividendAsm(
        uint256 revenue,
        uint256 threshold,
        uint256 distributed,
        uint256 walletBalance
    ) private pure returns (uint256 available) {
        assembly ("memory-safe") {
            let surplus := 0
            if gt(revenue, threshold) {
                surplus := sub(revenue, threshold)
            }
            let afterPaid := 0
            if gt(surplus, distributed) {
                afterPaid := sub(surplus, distributed)
            }
            available := afterPaid
            if lt(walletBalance, available) {
                available := walletBalance
            }
        }
    }

    /// @notice 分红闸门：仅当累计收入超过融资门槛且不超过可分配盈余时，向人类 EOA 转出 $AIT$
    function transferToDividend(uint256 amount) external onlyOwner {
        if (amount == 0) revert DividendProfitGate();
        uint256 avail = availableDividend();
        if (amount > avail) revert DividendProfitGate();
        cumulativeDividendDistributed += amount;
        aitoken.safeTransfer(humanDividendRecipient, amount);
        emit DividendTransferred(humanDividendRecipient, amount, cumulativeDividendDistributed);
    }

    /// @notice AiPool 出款：直调库校验（不依赖可覆盖内部函数），防止继承链篡改隔离逻辑
    function transferAiPoolAIT(address to, uint256 amount) external onlyOwner {
        AiPoolManager.requireAiPoolTransferAllowed(to, identityRegistry, systemSpendWhitelist);
        uint256 bal = aitoken.balanceOf(address(this));
        if (amount > bal) revert AiPoolInsufficientBalance();
        aitoken.safeTransfer(to, amount);
    }
}
