//! 结算服务：批量构造 `settleTrade` 输入；私钥从环境变量安全加载；可选 TEE 校验。
//!
//! **环境变量**：`SETTLER_PRIVATE_KEY` — 64 字符 hex（不含 0x），**切勿**写入仓库。

use crate::matching::Fill;
use crate::tee::{NoopTee, TeeAttestationProvider, TeeError};
use alloy_network::EthereumWallet;
use alloy_primitives::{hex, Address, Bytes, TxKind, B256, U256 as A256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::TransactionRequest;
use alloy_signer_local::PrivateKeySigner;
use alloy_sol_types::SolCall;
use k256::ecdsa::SigningKey;
use std::env;
use std::sync::Arc;
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// 单笔链上结算参数（对应 `InternalMarket.settleTrade`）
#[derive(Clone, Debug)]
pub struct SettleTradeCall {
    pub maker_order_id: u64,
    pub taker_order_id: u64,
    pub vstk_amount: alloy_primitives::U256,
    pub ait_notional: alloy_primitives::U256,
    pub settlement_id: B256,
}

#[derive(Debug, Error)]
pub enum SettlerError {
    #[error("SETTLER_PRIVATE_KEY missing or invalid")]
    InvalidPrivateKey,
    #[error("hex decode error: {0}")]
    Hex(#[from] hex::FromHexError),
    #[error("signing key error: {0}")]
    Key(String),
}

/// 内存安全：私钥在 drop 时清零（`ZeroizeOnDrop`）
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct ProtectedKey(Vec<u8>);

/// 从环境变量加载 hex 私钥（32 bytes）
pub fn load_private_key_from_env() -> Result<ProtectedKey, SettlerError> {
    let s = env::var("SETTLER_PRIVATE_KEY").map_err(|_| SettlerError::InvalidPrivateKey)?;
    let s = s.trim().strip_prefix("0x").unwrap_or(s.trim());
    if s.len() != 64 {
        return Err(SettlerError::InvalidPrivateKey);
    }
    let bytes = hex::decode(s)?;
    if bytes.len() != 32 {
        return Err(SettlerError::InvalidPrivateKey);
    }
    Ok(ProtectedKey(bytes))
}

/// 由原始字节构造 `SigningKey`（调用方负责保护 `key` 生命周期）
pub fn signing_key_from_protected(key: &ProtectedKey) -> Result<SigningKey, SettlerError> {
    SigningKey::from_bytes((&*key.0).into()).map_err(|e| SettlerError::Key(e.to_string()))
}

/// 将多笔成交转为批量结算调用（链上顺序提交）
pub fn batch_from_fills(fills: &[Fill], batch_entropy: B256) -> Vec<SettleTradeCall> {
    fills
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let mut hdata = Vec::new();
            hdata.extend_from_slice(batch_entropy.as_slice());
            hdata.extend_from_slice(&i.to_be_bytes());
            hdata.extend_from_slice(&f.maker_order_id.to_be_bytes());
            hdata.extend_from_slice(&f.taker_order_id.to_be_bytes());
            let settlement_id = alloy_primitives::keccak256(&hdata);
            SettleTradeCall {
                maker_order_id: f.maker_order_id,
                taker_order_id: f.taker_order_id,
                vstk_amount: f.vstk_amount,
                ait_notional: f.ait_notional,
                settlement_id: B256::from_slice(settlement_id.as_slice()),
            }
        })
        .collect()
}

/// 结算器配置（RPC / 合约地址在集成层填入）
#[derive(Clone)]
pub struct SettlerService {
    tee: Arc<dyn TeeAttestationProvider>,
}

impl Default for SettlerService {
    fn default() -> Self {
        Self::new()
    }
}

impl SettlerService {
    /// 使用空 TEE 占位
    pub fn new() -> Self {
        Self {
            tee: Arc::new(NoopTee),
        }
    }

    /// 注入 TEE 提供者（生产：远端 attestation）
    pub fn with_tee(tee: Arc<dyn TeeAttestationProvider>) -> Self {
        Self { tee }
    }

    /// 结算前可选：校验 TEE quote（未配置则跳过）
    pub async fn preflight_tee(&self) -> Result<(), TeeError> {
        match self.tee.fetch_quote().await {
            Ok(_) => Ok(()),
            Err(TeeError::NotConfigured) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

/// ABI 编码占位：`settleTrade` calldata 应由 `alloy-contract` 在集成 crate 中生成；此处仅保留函数选择器占位说明。
pub fn settle_trade_selector_reference() -> [u8; 4] {
    // settleTrade(uint256,uint256,uint256,uint256,bytes32)
    let sig = alloy_primitives::keccak256(b"settleTrade(uint256,uint256,uint256,uint256,bytes32)");
    let mut out = [0u8; 4];
    out.copy_from_slice(&sig[..4]);
    out
}

alloy_sol_types::sol! {
    #[derive(Debug)]
    contract InternalMarketSettle {
        function settleTrade(uint256 makerOrderId, uint256 takerOrderId, uint256 vstkAmount, uint256 aitNotional, bytes32 settlementId) external;
    }
}

#[derive(Debug, Error)]
pub enum SettlerTxError {
    #[error("RPC URL invalid")]
    BadRpc,
    #[error("signer: {0}")]
    Signer(String),
    #[error("provider: {0}")]
    Provider(String),
    #[error("tx send exhausted retries")]
    Exhausted,
}

/// 构造 `settleTrade` calldata（与 `InternalMarket.sol` 一致）
pub fn encode_settle_trade_calldata(call: &SettleTradeCall) -> Vec<u8> {
    InternalMarketSettle::settleTradeCall {
        makerOrderId: A256::from(call.maker_order_id),
        takerOrderId: A256::from(call.taker_order_id),
        vstkAmount: call.vstk_amount,
        aitNotional: call.ait_notional,
        settlementId: call.settlement_id,
    }
    .abi_encode()
}

/// 带 **指数退避** + **Gas 价格上抬** 的结算发送（Base / L2 EIP-1559）
pub async fn send_settle_trade_with_retry(
    rpc_url: &str,
    chain_id: u64,
    market: Address,
    signer: &PrivateKeySigner,
    call: &SettleTradeCall,
) -> Result<alloy_primitives::B256, SettlerTxError> {
    let url: reqwest::Url = rpc_url.parse().map_err(|_| SettlerTxError::BadRpc)?;
    let wallet = EthereumWallet::from(signer.clone());
    let provider = ProviderBuilder::new().wallet(wallet).on_http(url);

    let data = encode_settle_trade_calldata(call);
    let mut max_fee = provider
        .get_gas_price()
        .await
        .map_err(|e| SettlerTxError::Provider(e.to_string()))?;
    let mut priority = max_fee / 10u128;
    if priority == 0 {
        priority = 1;
    }

    const MAX_ATTEMPTS: u32 = 8;
    for attempt in 0..MAX_ATTEMPTS {
        let mut tx = TransactionRequest::default();
        tx.to = Some(TxKind::Call(market));
        tx.input = Bytes::from(data.clone()).into();
        tx.chain_id = Some(chain_id.into());
        tx.max_fee_per_gas = Some(max_fee);
        tx.max_priority_fee_per_gas = Some(priority);

        match provider.send_transaction(tx).await {
            Ok(pending) => {
                let hash = *pending.tx_hash();
                let _ = pending
                    .get_receipt()
                    .await
                    .map_err(|e| SettlerTxError::Provider(e.to_string()))?;
                return Ok(hash);
            }
            Err(e) => {
                tracing::warn!(attempt, %e, "settleTrade send failed, backoff + bump gas");
                max_fee = max_fee.saturating_mul(115u128) / 100u128;
                priority = priority.saturating_mul(115u128) / 100u128;
                let delay_ms = 150u64 * 2u64.pow(attempt.min(6));
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }
    }
    Err(SettlerTxError::Exhausted)
}

/// 从环境加载 `PrivateKeySigner`（`SETTLER_PRIVATE_KEY`）与链 ID（`CHAIN_ID`，默认 Base Sepolia 84532）
pub fn load_settler_signer() -> Result<(PrivateKeySigner, u64), SettlerError> {
    let key = load_private_key_from_env()?;
    let signer =
        PrivateKeySigner::from_slice(&key.0).map_err(|e| SettlerError::Key(e.to_string()))?;
    let chain_id: u64 = std::env::var("CHAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(84532);
    Ok((signer, chain_id))
}
