// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {Treasury} from "./Treasury.sol";

/// @title AI_Economist_Controller
/// @notice 唯一经授权对 `Treasury` 行使 `ECONOMIST_ROLE` 经济参数调整的入口；税率边界与调整间隔在链上强制（LAW 动态税收）
contract AI_Economist_Controller is Ownable {
    Treasury public immutable treasury;

    /// @notice 税率允许区间 [10,500] BPS，即 0.1% - 5.0%
    uint256 public constant MIN_TAX_BPS = 10;
    uint256 public constant MAX_TAX_BPS = 500;
    /// @notice 两次经济参数调整最小间隔（秒）：7 天
    uint256 public constant MIN_ADJUST_INTERVAL = 168 hours;

    uint256 public lastEconomistActionAt;

    event TaxRateProposed(uint256 indexed newRateBps, uint256 timestamp);
    event SalaryCoefficientUpdated(uint256 indexed newS, uint256 timestamp);

    error TaxRateOutOfRange();
    error EconomistCooldownActive();

    constructor(Treasury treasury_, address initialOwner) Ownable(initialOwner) {
        treasury = treasury_;
    }

    /// @dev Yul：高效判断 `last==0` 或 `block.timestamp >= last + interval`
    function _economistIntervalOpen(
        uint256 lastTs,
        uint256 currentTs,
        uint256 interval
    ) private pure returns (bool ok) {
        assembly ("memory-safe") {
            switch lastTs
            case 0 {
                ok := 1
            }
            default {
                ok := iszero(lt(currentTs, add(lastTs, interval)))
            }
        }
    }

    function _requireEconomistInterval() private view {
        if (!_economistIntervalOpen(lastEconomistActionAt, block.timestamp, MIN_ADJUST_INTERVAL)) {
            revert EconomistCooldownActive();
        }
    }

    /// @notice 提议并生效新税率（须由持有 `Treasury.ECONOMIST_ROLE` 的本合约调用）
    function proposeNewTaxRate(uint256 newRateBps) external onlyOwner {
        if (newRateBps < MIN_TAX_BPS || newRateBps > MAX_TAX_BPS) revert TaxRateOutOfRange();
        _requireEconomistInterval();
        treasury.setTaxRate(newRateBps);
        lastEconomistActionAt = block.timestamp;
        emit TaxRateProposed(newRateBps, block.timestamp);
    }

    /// @notice 更新周薪系数 S（与税率共享冷却窗口）
    function updateSalaryCoefficient(uint256 newS) external onlyOwner {
        _requireEconomistInterval();
        treasury.setWeeklySalaryCoefficient(newS);
        lastEconomistActionAt = block.timestamp;
        emit SalaryCoefficientUpdated(newS, block.timestamp);
    }
}
