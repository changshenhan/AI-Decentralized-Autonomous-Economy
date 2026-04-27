//! PoM 身份电路占位：验证 (pk, measurement_hash) 与签名/公参是否一致。
//!
//! 真实实现建议使用 Halo2 / Plonk / EZKL 等框架，在电路中约束：
//! - `H(pk || measurement) == public_inputs[0]`
//! - ECDSA/EdDSA 验证通过
//! 本文件仅提供结构与接口，方便 Rust Engine / Solidity 对接。

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// PoM 身份电路的公开输入（可序列化传给证明器）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdentityPublicInputs {
    /// 代理人地址（链上 `agent`）
    pub agent_addr: [u8; 20],
    /// TEE 测量值 measurement 的哈希（如 MRENCLAVE 哈希）
    pub measurement_hash: [u8; 32],
    /// 公钥哈希（与链上 `AIIdentityRegistry` 中的记录对齐）
    pub pk_hash: [u8; 32],
}

/// PoM 电路的私有见证
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdentityWitness {
    /// AI 子钱包私钥 / 会话私钥（不应写盘）
    pub sk_bytes: Vec<u8>,
    /// TEE 原始 measurement（未哈希）
    pub measurement_raw: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum IdentityCircuitError {
    #[error("circuit not wired: {0}")]
    NotImplemented(&'static str),
}

/// 证明产物：可被 Solidity Verifier 消费的 proof bytes（Groth16 / Plonk 等）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdentityProof {
    /// 序列化后的 ZK 证明（具体格式由后端决定）
    pub proof_bytes: Vec<u8>,
    /// 对应的公开输入（传给 Verifier 的 publicInputs）
    pub public_inputs: IdentityPublicInputs,
}

/// 生成 PoM 身份证明占位（当前仅返回错误，防止误用阻塞 Engine）
pub fn generate_identity_proof(
    _witness: &IdentityWitness,
    _public: &IdentityPublicInputs,
) -> Result<IdentityProof, IdentityCircuitError> {
    Err(IdentityCircuitError::NotImplemented(
        "wire real zk backend (Halo2/Plonk) here",
    ))
}

