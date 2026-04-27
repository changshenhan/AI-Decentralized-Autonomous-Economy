// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {AccessControl} from "@openzeppelin/contracts/access/AccessControl.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {IAIIdentityRegistry} from "./interfaces/IAIIdentityRegistry.sol";

/// @title AIIdentityRegistry
/// @notice PoM 机器身份白名单。生产路径：`attestMachineWithTeeSignature`（受信 TEE 节点 ECDSA，过渡方案，避免重型 ZK Verifier）
contract AIIdentityRegistry is IAIIdentityRegistry, AccessControl {
    bytes32 public constant VERIFIER_ROLE = keccak256("VERIFIER_ROLE");

    /// @notice EIP-712 风格结构化哈希前缀（与链下 `ethers.TypedDataEncoder` / abi.encode 对齐）
    bytes32 public constant TEE_ATTEST_TYPEHASH =
        keccak256("AIDE_TEE_ATTEST_V1(address agent,uint256 deadline,bytes32 salt,uint256 chainId,address registry)");

    /// @notice 受信 TEE 协调节点公钥地址（仅该密钥签名的身份证明有效）
    address public immutable teeAuthority;

    mapping(address => bool) private _verified;
    mapping(address => bytes32) private _lastProof;

    event MachineVerified(address indexed agent, bytes32 proof);
    event MachineRevoked(address indexed agent);
    event TeeMachineAttested(address indexed agent, uint256 deadline, bytes32 salt, bytes32 digest);
    event ZkIdentityDeprecated(address indexed caller);

    error ZeroAddress();
    error BadTeeSignature();
    error ExpiredDeadline();
    error ZkPathDeprecated();

    constructor(address admin, address teeAuthority_) {
        if (admin == address(0) || teeAuthority_ == address(0)) revert ZeroAddress();
        teeAuthority = teeAuthority_;
        _grantRole(DEFAULT_ADMIN_ROLE, admin);
        _grantRole(VERIFIER_ROLE, admin);
    }

    /// @notice 人工/治理登记（保留；`proof` 为审计锚点哈希）
    function verifyMachine(address agent, bytes32 proof) external onlyRole(VERIFIER_ROLE) {
        require(agent != address(0), "AIIdentity: zero agent");
        require(proof != bytes32(0), "AIIdentity: empty proof");
        _verified[agent] = true;
        _lastProof[agent] = proof;
        emit MachineVerified(agent, proof);
    }

    /// @notice 已废除：请使用 `attestMachineWithTeeSignature`
    function verifyZkIdentity(address, bytes calldata, bytes32[] calldata) external pure {
        revert ZkPathDeprecated();
    }

    /// @notice TEE 节点对 `(agent, deadline, salt, chainId, registry)` 结构化摘要签名；链上恢复地址须等于 `teeAuthority`
    function attestMachineWithTeeSignature(
        address agent,
        uint256 deadline,
        bytes32 salt,
        bytes calldata signature
    ) external {
        if (agent == address(0)) revert ZeroAddress();
        if (block.timestamp > deadline) revert ExpiredDeadline();

        bytes32 structHash = keccak256(abi.encode(TEE_ATTEST_TYPEHASH, agent, deadline, salt, block.chainid, address(this)));
        bytes32 digest = MessageHashUtils.toEthSignedMessageHash(structHash);
        address recovered = ECDSA.recover(digest, signature);
        if (recovered != teeAuthority) revert BadTeeSignature();

        _verified[agent] = true;
        _lastProof[agent] = keccak256(abi.encodePacked("TEE_ECDSA_V1", structHash));
        emit TeeMachineAttested(agent, deadline, salt, structHash);
    }

    function revokeMachine(address agent) external onlyRole(DEFAULT_ADMIN_ROLE) {
        _verified[agent] = false;
        emit MachineRevoked(agent);
    }

    function lastProof(address agent) external view returns (bytes32) {
        return _lastProof[agent];
    }

    /// @inheritdoc IAIIdentityRegistry
    function isVerifiedMachine(address account) external view override returns (bool) {
        return _verified[account];
    }
}
