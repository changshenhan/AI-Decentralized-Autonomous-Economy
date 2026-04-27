// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IAIIdentityRegistry
/// @notice PoM / 机器身份登记（LAW：仅验证机器可向 AiPool 外指定路径收款）
interface IAIIdentityRegistry {
    function isVerifiedMachine(address account) external view returns (bool);
}
