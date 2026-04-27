// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IGuardrail
/// @notice 全局熔断：国库异常流出、单笔税额相对名义异常；与 Treasury / InternalMarket 协同（第九阶段）
interface IGuardrail {
    function paused() external view returns (bool);

    function requireNotPaused() external view;

    /// @notice 若单笔运营支出超过当前余额的 `outflowBpsThreshold` / 10000，则进入 PAUSE 并返回 false（调用方须在未改状态前 return）
    function evaluateOutflow(uint256 balanceBefore, uint256 amount) external returns (bool allowed);

    /// @notice 若 fee 相对名义额超过 `maxFeeBpsOfNotional`，则 PAUSE 并返回 false
    function evaluateSettleFee(uint256 fee, uint256 aitNotional) external returns (bool allowed);

    function setPeers(address treasury_, address internalMarket_) external;
}
