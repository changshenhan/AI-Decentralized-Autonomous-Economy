// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {AccessControl} from "@openzeppelin/contracts/access/AccessControl.sol";
import {IBankAudit} from "./interfaces/IBankAudit.sol";

/// @title AIBank
/// @notice AI 投行准入：`approveListing` 后 `InternalMarket` 方可撮合该公司 vSTK；`codeAuditHash` 锚定链下业务/代码审计
contract AIBank is AccessControl, IBankAudit {
    bytes32 public constant AUDITOR_ROLE = keccak256("AUDITOR_ROLE");

    mapping(uint256 => bool) private _tradable;
    mapping(uint256 => bytes32) private _codeAuditHash;

    event ListingApproved(uint256 indexed companyId, bytes32 codeAuditHash, address indexed auditor);
    event ListingRevoked(uint256 indexed companyId, address indexed admin);

    error ZeroCompanyId();

    constructor(address admin) {
        require(admin != address(0), "AIBank: zero admin");
        _grantRole(DEFAULT_ADMIN_ROLE, admin);
        _grantRole(AUDITOR_ROLE, admin);
    }

    /// @notice 通过投行审计后，标记公司 vSTK 可在 `InternalMarket` 交易
    function approveListing(uint256 companyId, bytes32 codeAuditHash_) external onlyRole(AUDITOR_ROLE) {
        if (companyId == 0) revert ZeroCompanyId();
        _tradable[companyId] = true;
        _codeAuditHash[companyId] = codeAuditHash_;
        emit ListingApproved(companyId, codeAuditHash_, msg.sender);
    }

    function revokeListing(uint256 companyId) external onlyRole(DEFAULT_ADMIN_ROLE) {
        _tradable[companyId] = false;
        emit ListingRevoked(companyId, msg.sender);
    }

    /// @inheritdoc IBankAudit
    function isTradable(uint256 companyId) external view override returns (bool) {
        return _tradable[companyId];
    }

    /// @inheritdoc IBankAudit
    function codeAuditHash(uint256 companyId) external view override returns (bytes32) {
        return _codeAuditHash[companyId];
    }
}
