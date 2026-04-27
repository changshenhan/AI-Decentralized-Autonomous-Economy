// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {AccessControl} from "@openzeppelin/contracts/access/AccessControl.sol";
import {IGuardrail} from "./interfaces/IGuardrail.sol";

/// @title Guardrail
/// @notice 系统熔断器：监视国库大额流出与结算税额异常；仅管理员或紧急委员会可 unpause（ZK 委员会角色预留）
contract Guardrail is AccessControl, IGuardrail {
    bytes32 public constant EMERGENCY_COMMITTEE_ROLE = keccak256("EMERGENCY_COMMITTEE_ROLE");

    /// @notice 国库地址（`evaluateOutflow` 仅 Treasury 可调）
    address public treasury;

    /// @notice InternalMarket（`evaluateSettleFee` 仅该地址可调）
    address public internalMarket;

    bool public override paused;

    /// @notice 单笔流出超过 `balanceBefore * outflowBpsThreshold / 10000` 时熔断（默认 30%）
    uint256 public outflowBpsThreshold = 3000;

    /// @notice fee 不得超过 `aitNotional * maxFeeBpsOfNotional / 10000`（默认 500 = 5%，与 AI_Economist_Controller 上限一致）
    uint256 public maxFeeBpsOfNotional = 500;

    event PeersWired(address indexed treasury, address indexed internalMarket);
    event CircuitBroken(bytes32 indexed reason, uint256 detail);
    event CircuitUnpaused(address indexed by);
    event OutflowThresholdUpdated(uint256 bps);
    event MaxFeeBpsUpdated(uint256 bps);

    error SystemPaused();
    error OnlyTreasury();
    error OnlyInternalMarket();
    error PeersAlreadyWired();
    error ZeroPeer();
    error UnauthorizedUnpause();

    constructor(address admin) {
        require(admin != address(0), "Guardrail: zero admin");
        _grantRole(DEFAULT_ADMIN_ROLE, admin);
        _grantRole(EMERGENCY_COMMITTEE_ROLE, admin);
    }

    function setPeers(address treasury_, address internalMarket_) external override onlyRole(DEFAULT_ADMIN_ROLE) {
        if (treasury != address(0)) revert PeersAlreadyWired();
        if (treasury_ == address(0) || internalMarket_ == address(0)) revert ZeroPeer();
        treasury = treasury_;
        internalMarket = internalMarket_;
        emit PeersWired(treasury_, internalMarket_);
    }

    function setOutflowBpsThreshold(uint256 bps) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(bps <= 10_000, "Guardrail: bps");
        outflowBpsThreshold = bps;
        emit OutflowThresholdUpdated(bps);
    }

    function setMaxFeeBpsOfNotional(uint256 bps) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(bps <= 10_000, "Guardrail: bps");
        maxFeeBpsOfNotional = bps;
        emit MaxFeeBpsUpdated(bps);
    }

    function requireNotPaused() external view override {
        if (paused) revert SystemPaused();
    }

    function evaluateOutflow(uint256 balanceBefore, uint256 amount) external override returns (bool allowed) {
        if (msg.sender != treasury) revert OnlyTreasury();
        if (paused) return false;
        if (balanceBefore > 0 && amount * 10_000 > balanceBefore * outflowBpsThreshold) {
            paused = true;
            emit CircuitBroken(keccak256("OUTFLOW"), amount);
            return false;
        }
        return true;
    }

    function evaluateSettleFee(uint256 fee, uint256 aitNotional) external override returns (bool allowed) {
        if (msg.sender != internalMarket) revert OnlyInternalMarket();
        if (paused) return false;
        if (aitNotional > 0 && fee * 10_000 > aitNotional * maxFeeBpsOfNotional) {
            paused = true;
            emit CircuitBroken(keccak256("FEE"), fee);
            return false;
        }
        return true;
    }

    /// @notice 管理员或紧急委员会可解除暂停（ZK 证明路径可后续替换为仅委员会）
    function unpause() external {
        if (!hasRole(DEFAULT_ADMIN_ROLE, msg.sender) && !hasRole(EMERGENCY_COMMITTEE_ROLE, msg.sender)) {
            revert UnauthorizedUnpause();
        }
        paused = false;
        emit CircuitUnpaused(msg.sender);
    }
}
