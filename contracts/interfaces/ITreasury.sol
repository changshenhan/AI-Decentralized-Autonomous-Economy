// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title ITreasury
/// @notice 国库视图接口：动态税率查询（LAW 第5条、.cursorrules 抽税公式）
interface ITreasury {
    function getCurrentTaxRate() external view returns (uint256 taxRateBps);

    /// @notice 协议结算显式抽税：`fee = taxableBase * rate / 10000`，从 `payer` pull 至国库
    function collectTax(address payer, uint256 taxableBase) external returns (uint256 fee);
}
