// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IAIIdentityRegistry} from "./interfaces/IAIIdentityRegistry.sol";

/// @title AiPoolManager
/// @notice 双池之 AiPool 侧：库 + 可继承基类。硬编码拒绝向「非机器验证」且非系统白名单的地址汇出（LAW 第2条、第8条、开发者指令第2条）
library AiPoolManager {
    error AiPoolZeroRecipient();
    error AiPoolHumanOrUnverifiedRecipient(address to);

    /// @param registry PoM / 链上机器登记；`isVerifiedMachine(to)==true` 表示允许作为 AiPool 收款方
    /// @param systemSpendWhitelist 算力/API/协议结算合约白名单（非人类 EOA，由国库/治理维护）
    function requireAiPoolTransferAllowed(
        address to,
        IAIIdentityRegistry registry,
        mapping(address => bool) storage systemSpendWhitelist
    ) internal view {
        if (to == address(0)) revert AiPoolZeroRecipient();
        if (registry.isVerifiedMachine(to)) return;
        if (systemSpendWhitelist[to]) return;
        revert AiPoolHumanOrUnverifiedRecipient(to);
    }
}

/// @title AiPoolVaultBase
/// @notice AI 钱包合约应继承此类，在划出 AiPool 资金前调用 `_enforceAiPoolRecipient`
abstract contract AiPoolVaultBase is IAIIdentityRegistry {
    IAIIdentityRegistry public immutable identityRegistry;
    mapping(address => bool) public systemSpendWhitelist;

    event SystemSpendWhitelistUpdated(address indexed target, bool allowed);

    constructor(IAIIdentityRegistry registry_) {
        identityRegistry = registry_;
    }

    /// @notice 由治理/国库角色维护的协议收款方（禁止将人类 EOA 加入白名单）
    function _setSystemSpend(address target, bool allowed) internal {
        systemSpendWhitelist[target] = allowed;
        emit SystemSpendWhitelistUpdated(target, allowed);
    }

    /// @dev 子合约在任意减少 AiPool 记账余额并发起对外转账前必须调用
    function _enforceAiPoolRecipient(address to) internal view virtual {
        AiPoolManager.requireAiPoolTransferAllowed(to, identityRegistry, systemSpendWhitelist);
    }

    /// @inheritdoc IAIIdentityRegistry
    function isVerifiedMachine(address account) public view virtual override returns (bool) {
        return identityRegistry.isVerifiedMachine(account);
    }
}
