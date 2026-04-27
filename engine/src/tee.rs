//! TEE（可信执行环境）远端度量占位：生产可将结算私钥与签名约束在 SGX/SEV 内执行。
use async_trait::async_trait;
use thiserror::Error;

/// TEE 度量或 attestation 获取失败
#[derive(Debug, Error)]
pub enum TeeError {
    #[error("tee provider not configured")]
    NotConfigured,
    #[error("remote attestation failed: {0}")]
    Remote(String),
}

/// 预留：向远端 TEE 或 KMS 请求 quote / 健康证明
#[async_trait]
pub trait TeeAttestationProvider: Send + Sync {
    /// 拉取硬件 attestation quote（字节格式依平台而定）
    async fn fetch_quote(&self) -> Result<Vec<u8>, TeeError>;
}

/// 默认空实现（开发/CI）
pub struct NoopTee;

#[async_trait]
impl TeeAttestationProvider for NoopTee {
    async fn fetch_quote(&self) -> Result<Vec<u8>, TeeError> {
        Err(TeeError::NotConfigured)
    }
}
