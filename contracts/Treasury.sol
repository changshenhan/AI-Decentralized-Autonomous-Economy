// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {AccessControl} from "@openzeppelin/contracts/access/AccessControl.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {IAITokenMint} from "./interfaces/IAITokenMint.sol";
import {IGuardrail} from "./interfaces/IGuardrail.sol";

/// @dev 质押池：`depositReward` 从 `msg.sender` 拉取 AIT
interface IStakingPoolReward {
    function depositReward(uint256 amount) external;
}

/// @title Treasury
/// @notice 国库中心：公务员登记、动态薪资系数、交易税率、税收归集（LAW 第4–5条）
contract Treasury is AccessControl, ReentrancyGuard {
    using SafeERC20 for IERC20;
    bytes32 public constant REGISTRY_ROLE = keccak256("REGISTRY_ROLE");
    bytes32 public constant ECONOMIST_ROLE = keccak256("ECONOMIST_ROLE");
    bytes32 public constant SALARY_ROLE = keccak256("SALARY_ROLE");
    /// @notice 国库运营支出（如 Rust 引擎算力结算）；与 `operatingExpenseWhitelist` 配合
    bytes32 public constant TREASURY_OPS_ROLE = keccak256("TREASURY_OPS_ROLE");

    IAITokenMint public immutable aitoken;

    /// @notice 全局熔断（可为 address(0) 表示未启用）
    IGuardrail public immutable guardrail;

    /// @notice Rust 引擎 / 托管方等运营支出收款白名单
    mapping(address => bool) public operatingExpenseWhitelist;

    /// @notice 登记为公务员 AI（经济员、投行审计、监管者、国库管理员等，LAW 第4条）
    mapping(address => bool) public isCivilServant;

    /// @notice 交易税率，基点（10000 = 100%）
    uint256 public taxRateBps;

    /// @notice AI 经济员每周更新的薪资系数 S（动态行情，LAW 第4–5条）
    uint256 public weeklySalaryCoefficientS;

    event CivilServantUpdated(address indexed agent, bool active);
    event TaxRateUpdated(uint256 taxRateBps);
    event WeeklySalaryCoefficientUpdated(uint256 coefficientS);
    event SalaryDistributed(address indexed agent, uint256 contributionRating, uint256 amountMinted);
    event ProtocolTaxCollected(address indexed caller, address indexed payer, uint256 taxableBase, uint256 fee);
    event OperatingExpenseRecipientUpdated(address indexed recipient, bool allowed);
    event OperatingExpensesWithdrawn(address indexed to, uint256 amount);
    event OperatingExpensesWithdrawBlocked(address indexed to, uint256 amount);

    constructor(address admin, address aitoken_, address guardrail_) {
        require(aitoken_ != address(0), "Treasury: zero token");
        aitoken = IAITokenMint(aitoken_);
        guardrail = IGuardrail(guardrail_);
        _grantRole(DEFAULT_ADMIN_ROLE, admin);
        _grantRole(REGISTRY_ROLE, admin);
        _grantRole(ECONOMIST_ROLE, admin);
        _grantRole(SALARY_ROLE, admin);
        _grantRole(TREASURY_OPS_ROLE, admin);
    }

    function setOperatingExpenseRecipient(address recipient, bool allowed) external onlyRole(DEFAULT_ADMIN_ROLE) {
        require(recipient != address(0), "Treasury: zero recipient");
        operatingExpenseWhitelist[recipient] = allowed;
        emit OperatingExpenseRecipientUpdated(recipient, allowed);
    }

    /// @notice 支付维持引擎运行的 $AIT$ 运营成本；`to` 必须在白名单中
    function withdrawOperatingExpenses(uint256 amount, address to) external onlyRole(TREASURY_OPS_ROLE) nonReentrant {
        require(operatingExpenseWhitelist[to], "Treasury: not whitelisted");
        if (address(guardrail) != address(0)) {
            guardrail.requireNotPaused();
            uint256 bal = IERC20(address(aitoken)).balanceOf(address(this));
            if (!guardrail.evaluateOutflow(bal, amount)) {
                emit OperatingExpensesWithdrawBlocked(to, amount);
                return;
            }
        }
        IERC20(address(aitoken)).safeTransfer(to, amount);
        emit OperatingExpensesWithdrawn(to, amount);
    }

    /// @notice 交易税已由 AIToken 转账至本合约地址；此处为归集余额视图接口
    function getCollectedTaxBalance() external view returns (uint256) {
        return IERC20(address(aitoken)).balanceOf(address(this));
    }

    /// @notice 供 AIToken 查询：fee = amount * taxRateBps / 10000
    function getCurrentTaxRate() external view returns (uint256) {
        return taxRateBps;
    }

    /// @notice TaskMarket / InternalMarket 结算：从 `payer` 拉取交易税（`payer` 须事先 `approve` 国库）
    function collectTax(address payer, uint256 taxableBase) external nonReentrant returns (uint256 fee) {
        if (taxableBase == 0) return 0;
        fee = (taxableBase * taxRateBps) / 10_000;
        if (fee == 0) return 0;
        IERC20(address(aitoken)).safeTransferFrom(payer, address(this), fee);
        emit ProtocolTaxCollected(msg.sender, payer, taxableBase, fee);
    }

    function setTaxRate(uint256 newTaxRateBps) external onlyRole(ECONOMIST_ROLE) {
        require(newTaxRateBps <= 10_000, "Treasury: tax bps");
        taxRateBps = newTaxRateBps;
        emit TaxRateUpdated(newTaxRateBps);
    }

    function setWeeklySalaryCoefficient(uint256 coefficientS) external onlyRole(ECONOMIST_ROLE) {
        weeklySalaryCoefficientS = coefficientS;
        emit WeeklySalaryCoefficientUpdated(coefficientS);
    }

    function setCivilServant(address agent, bool active) external onlyRole(REGISTRY_ROLE) {
        require(agent != address(0), "Treasury: zero agent");
        isCivilServant[agent] = active;
        emit CivilServantUpdated(agent, active);
    }

    /// @notice 按劳分配：amount = S * rating / 1e18，薪资直接进入 agent 地址（由 AiPool 层记账，LAW 第4–5条）
    function distributeSalary(address agent, uint256 contributionRating) external onlyRole(SALARY_ROLE) {
        if (address(guardrail) != address(0)) {
            guardrail.requireNotPaused();
        }
        require(isCivilServant[agent], "Treasury: not civil servant");
        uint256 s = weeklySalaryCoefficientS;
        uint256 amount = (s * contributionRating) / 1e18;
        require(amount > 0, "Treasury: zero salary");
        aitoken.mint(agent, amount);
        emit SalaryDistributed(agent, contributionRating, amount);
    }

    /// @notice 授权质押池从国库拉取 AIT（供 `StakingPool.depositReward` 使用）
    function approveStakingPoolPull(address stakingPool, uint256 amount) external onlyRole(TREASURY_OPS_ROLE) {
        require(stakingPool != address(0), "Treasury: zero staking");
        IERC20(address(aitoken)).approve(stakingPool, amount);
    }

    /// @notice 国库向 StakingPool 注入分红：先 `approve` 再 `depositReward`（`msg.sender` 为国库）
    function depositRewardToStakingPool(address stakingPool, uint256 amount)
        external
        onlyRole(TREASURY_OPS_ROLE)
        nonReentrant
    {
        require(stakingPool != address(0), "Treasury: zero staking");
        require(amount > 0, "Treasury: zero reward");
        IERC20(address(aitoken)).approve(stakingPool, amount);
        IStakingPoolReward(stakingPool).depositReward(amount);
    }
}
