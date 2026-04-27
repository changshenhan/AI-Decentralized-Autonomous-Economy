// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {IAIIdentityRegistry} from "./interfaces/IAIIdentityRegistry.sol";
import {Treasury} from "./Treasury.sol";
import {IGuardrail} from "./interfaces/IGuardrail.sol";

/// @title TaskMarket
/// @notice 生产力中心：Escrow 悬赏、验证后结算；抽税经 `Treasury.collectTax`；赏金仅释放至已验证机器的 AiPool 收款地址（LAW 第2、8 条）
contract TaskMarket is Ownable, ReentrancyGuard {
    using SafeERC20 for IERC20;

    IERC20 public immutable aitoken;
    Treasury public immutable treasury;
    IAIIdentityRegistry public immutable registry;

    /// @notice 全局熔断（可为 address(0)）
    IGuardrail public immutable guardrail;

    uint256 public nextTaskId = 1;

    enum Status {
        Open,
        Funded,
        Assigned,
        Submitted,
        Completed,
        Cancelled
    }

    struct Task {
        address publisher;
        uint256 bounty;
        uint256 lockedAmount;
        address worker;
        address aiPoolRecipient;
        Status status;
        bytes32 resultHash;
        bytes32 zkProofCommitment;
    }

    mapping(uint256 => Task) public tasks;

    event TaskPosted(uint256 indexed taskId, address indexed publisher, uint256 bounty);
    event TaskFunded(uint256 indexed taskId, uint256 lockedAmount);
    event WorkerAssigned(
        uint256 indexed taskId,
        address indexed worker,
        address indexed aiPoolRecipient
    );
    event WorkSubmitted(uint256 indexed taskId, address indexed worker, bytes32 resultHash);
    event ZkProofCommitted(uint256 indexed taskId, bytes32 commitment);
    event TaskApproved(uint256 indexed taskId, address indexed approver);
    event TaskSettled(
        uint256 indexed taskId,
        address indexed aiPoolRecipient,
        uint256 grossAmount,
        uint256 fee,
        uint256 netToAiPool
    );
    event TaskCancelled(uint256 indexed taskId);

    error BadStatus();
    error NotPublisher();
    error NotWorker();
    error AiPoolRecipientNotVerified();
    error ZeroAddress();

    constructor(
        IERC20 aitoken_,
        Treasury treasury_,
        IAIIdentityRegistry registry_,
        address initialOwner,
        address guardrail_
    ) Ownable(initialOwner) {
        if (address(aitoken_) == address(0) || address(treasury_) == address(0) || address(registry_) == address(0)) {
            revert ZeroAddress();
        }
        aitoken = aitoken_;
        treasury = treasury_;
        registry = registry_;
        guardrail = IGuardrail(guardrail_);
        SafeERC20.forceApprove(IERC20(address(aitoken_)), address(treasury_), type(uint256).max);
    }

    /// @notice 发布任务（先 post 再 fund，或仅记录意向）
    function postTask(uint256 bounty) external returns (uint256 taskId) {
        taskId = nextTaskId++;
        tasks[taskId] = Task({
            publisher: msg.sender,
            bounty: bounty,
            lockedAmount: 0,
            worker: address(0),
            aiPoolRecipient: address(0),
            status: Status.Open,
            resultHash: bytes32(0),
            zkProofCommitment: bytes32(0)
        });
        emit TaskPosted(taskId, msg.sender, bounty);
    }

    /// @notice 发布者存入 $AIT 进入托管
    function fundTask(uint256 taskId) external nonReentrant {
        Task storage t = tasks[taskId];
        if (t.publisher != msg.sender) revert NotPublisher();
        if (t.status != Status.Open) revert BadStatus();
        uint256 amt = t.bounty;
        if (amt == 0) revert BadStatus();
        aitoken.safeTransferFrom(msg.sender, address(this), amt);
        t.lockedAmount = amt;
        t.status = Status.Funded;
        emit TaskFunded(taskId, amt);
    }

    /// @notice 绑定执行者与 **AiPool 收款地址**（须 PoM 验证）；禁止人类地址
    function assignWorker(uint256 taskId, address worker, address aiPoolRecipient_) external {
        Task storage t = tasks[taskId];
        if (t.publisher != msg.sender) revert NotPublisher();
        if (t.status != Status.Funded) revert BadStatus();
        if (!registry.isVerifiedMachine(aiPoolRecipient_)) revert AiPoolRecipientNotVerified();
        t.worker = worker;
        t.aiPoolRecipient = aiPoolRecipient_;
        t.status = Status.Assigned;
        emit WorkerAssigned(taskId, worker, aiPoolRecipient_);
    }

    function submitWork(uint256 taskId, bytes32 resultHash) external {
        Task storage t = tasks[taskId];
        if (t.status != Status.Assigned) revert BadStatus();
        if (msg.sender != t.worker) revert NotWorker();
        t.resultHash = resultHash;
        t.status = Status.Submitted;
        emit WorkSubmitted(taskId, msg.sender, resultHash);
    }

    /// @notice 预留：未来 ZK 证明承诺上链，由链下验证器核对
    function commitZkProof(uint256 taskId, bytes32 commitment) external {
        Task storage t = tasks[taskId];
        if (t.status != Status.Submitted && t.status != Status.Assigned) revert BadStatus();
        if (msg.sender != t.worker && msg.sender != t.publisher) revert NotWorker();
        t.zkProofCommitment = commitment;
        emit ZkProofCommitted(taskId, commitment);
    }

    /// @notice 发布者确认交付（多签场景可替换为 verifier 集合）
    function approveTask(uint256 taskId) external {
        Task storage t = tasks[taskId];
        if (t.publisher != msg.sender) revert NotPublisher();
        if (t.status != Status.Submitted) revert BadStatus();
        emit TaskApproved(taskId, msg.sender);
        _settle(taskId);
    }

    /// @notice 抽税 + 释放至 AiPool 收款地址（结算路径对 AIT 免自动税，由 collectTax 显式抽税）
    function _settle(uint256 taskId) internal {
        Task storage t = tasks[taskId];
        uint256 gross = t.lockedAmount;
        if (gross == 0) revert BadStatus();
        if (!registry.isVerifiedMachine(t.aiPoolRecipient)) revert AiPoolRecipientNotVerified();

        if (address(guardrail) != address(0)) {
            guardrail.requireNotPaused();
        }

        uint256 fee = treasury.collectTax(address(this), gross);
        uint256 net = gross - fee;
        t.lockedAmount = 0;
        t.status = Status.Completed;
        aitoken.safeTransfer(t.aiPoolRecipient, net);
        emit TaskSettled(taskId, t.aiPoolRecipient, gross, fee, net);
    }

    function cancelTask(uint256 taskId) external nonReentrant {
        Task storage t = tasks[taskId];
        if (t.publisher != msg.sender) revert NotPublisher();
        if (t.status == Status.Completed || t.status == Status.Cancelled) revert BadStatus();
        uint256 locked = t.lockedAmount;
        t.status = Status.Cancelled;
        t.lockedAmount = 0;
        if (locked > 0) {
            aitoken.safeTransfer(t.publisher, locked);
        }
        emit TaskCancelled(taskId);
    }
}
