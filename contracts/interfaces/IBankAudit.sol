// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IBankAudit
/// @notice AI 投行审计视图：上市准入与代码审计哈希锚定（LAW：虚假上市禁止）
interface IBankAudit {
    function isTradable(uint256 companyId) external view returns (bool);

    function codeAuditHash(uint256 companyId) external view returns (bytes32);
}
