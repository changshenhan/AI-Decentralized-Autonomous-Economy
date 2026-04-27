//! Sync-Audit：周期性拉取链上 `InternalMarket` / `Treasury` 视图，与 `SyncCoordinator` 影子状态比对（ENGINE_SYNC_SPEC）

use crate::matching::MatchEngine;
use crate::sync::{ChainBalanceSnapshot, ReconciliationReport, SyncCoordinator};
use alloy_network::Ethereum;
use alloy_primitives::{Address, Bytes, TxKind, U256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::TransactionRequest;
use alloy_sol_types::SolCall;
use alloy_transport_http::Http;
use reqwest::Client;
use thiserror::Error;
use tracing::{error, info, warn};

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("bad rpc url")]
    BadRpc,
    #[error("eth_call failed: {0}")]
    Call(String),
    #[error("return data too short")]
    Decode,
}

alloy_sol_types::sol! {
    contract IInternalMarketView {
        function aitBalance(address user) external view returns (uint256);
        function vstkBalance(uint256 companyId, address user) external view returns (uint256);
    }
    contract ITreasuryView {
        function getCollectedTaxBalance() external view returns (uint256);
        function getCurrentTaxRate() external view returns (uint256);
    }
}

fn decode_u256_word(data: &[u8]) -> Result<U256, AuditError> {
    if data.len() < 32 {
        return Err(AuditError::Decode);
    }
    Ok(U256::from_be_slice(&data[data.len() - 32..]))
}

/// 拉取影子键对应的链上余额，并附带国库已归集税收读数（遥测 / 人工对账）
pub async fn fetch_chain_snapshot_for_shadow<P>(
    provider: &P,
    market: Address,
    treasury: Address,
    sync: &SyncCoordinator,
) -> Result<(ChainBalanceSnapshot, U256), AuditError>
where
    P: Provider<Http<Client>, Ethereum> + Clone,
{
    let snap = ChainBalanceSnapshot::new();

    for user in sync.shadow_ait_users() {
        let call = IInternalMarketView::aitBalanceCall { user };
        let mut tx = TransactionRequest::default();
        tx.to = Some(TxKind::Call(market));
        tx.input = Bytes::from(call.abi_encode()).into();
        let out = provider
            .call(&tx)
            .await
            .map_err(|e| AuditError::Call(e.to_string()))?;
        let v = decode_u256_word(out.as_ref())?;
        snap.ait.insert(user, v);
    }

    for (company_id, user) in sync.shadow_vstk_keys() {
        let call = IInternalMarketView::vstkBalanceCall {
            companyId: U256::from(company_id),
            user,
        };
        let mut tx = TransactionRequest::default();
        tx.to = Some(TxKind::Call(market));
        tx.input = Bytes::from(call.abi_encode()).into();
        let out = provider
            .call(&tx)
            .await
            .map_err(|e| AuditError::Call(e.to_string()))?;
        let v = decode_u256_word(out.as_ref())?;
        snap.vstk.insert((company_id, user), v);
    }

    let tax_call = ITreasuryView::getCollectedTaxBalanceCall {};
    let mut tx = TransactionRequest::default();
    tx.to = Some(TxKind::Call(treasury));
    tx.input = Bytes::from(tax_call.abi_encode()).into();
    let tout = provider
        .call(&tx)
        .await
        .map_err(|e| AuditError::Call(e.to_string()))?;
    let tax_on_chain = decode_u256_word(tout.as_ref())?;

    Ok((snap, tax_on_chain))
}

/// 从国库只读同步 `taxRateBps` 至撮合引擎（与 `Treasury.getCurrentTaxRate` 一致）
pub async fn sync_treasury_tax_rate_to_engine<P>(
    provider: &P,
    treasury: Address,
    engine: &MatchEngine,
) -> Result<u64, AuditError>
where
    P: Provider<Http<Client>, Ethereum> + Clone,
{
    let call = ITreasuryView::getCurrentTaxRateCall {};
    let mut tx = TransactionRequest::default();
    tx.to = Some(TxKind::Call(treasury));
    tx.input = Bytes::from(call.abi_encode()).into();
    let out = provider
        .call(&tx)
        .await
        .map_err(|e| AuditError::Call(e.to_string()))?;
    let v = decode_u256_word(out.as_ref())?;
    let bps_u128: u128 = v.to::<u128>();
    let bps = (bps_u128.min(u64::MAX as u128)) as u64;
    engine.set_tax_rate_bps(bps);
    Ok(bps)
}

/// 若 `threshold == 0`，任意非零差即视为漂移；否则要求 `abs(local - chain) <= threshold`
pub fn report_exceeds_threshold(report: &ReconciliationReport, threshold: U256) -> bool {
    for d in &report.drifts {
        let diff = if d.local > d.chain {
            d.local - d.chain
        } else {
            d.chain - d.local
        };
        if threshold.is_zero() {
            if diff > U256::ZERO {
                return true;
            }
        } else if diff > threshold {
            return true;
        }
    }
    false
}

/// 单次对账：`true` = 影子与链一致（在阈值内）
pub async fn run_sync_audit_once(
    rpc_url: &str,
    market: Address,
    treasury: Address,
    sync: &SyncCoordinator,
    engine: &MatchEngine,
    threshold: U256,
) -> Result<bool, AuditError> {
    let url: reqwest::Url = rpc_url.parse().map_err(|_| AuditError::BadRpc)?;
    let provider = ProviderBuilder::new().on_http(url);
    match sync_treasury_tax_rate_to_engine(&provider, treasury, engine).await {
        Ok(bps) => {
            info!(
                target: "aide_audit",
                tax_bps = bps,
                "Treasury.getCurrentTaxRate synced to MatchEngine"
            );
        }
        Err(e) => {
            warn!(
                target: "aide_audit",
                ?e,
                "Treasury.getCurrentTaxRate sync failed; MatchEngine tax_rate_bps may be stale"
            );
        }
    }
    let (snap, tax_collected) =
        fetch_chain_snapshot_for_shadow(&provider, market, treasury, sync).await?;
    info!(
        target: "aide_audit",
        tax_wei = %tax_collected,
        "Sync-Audit: chain snapshot (Treasury.getCollectedTaxBalance + InternalMarket balances)"
    );
    let report = sync.reconcile(&snap);
    if report.drifts.is_empty() {
        return Ok(true);
    }
    if report_exceeds_threshold(&report, threshold) {
        error!(
            target: "aide_audit",
            "[CRITICAL_DRIFT] shadow vs chain: {} entries (threshold={})",
            report.drifts.len(),
            threshold
        );
        for d in report.drifts.iter().take(8) {
            error!(
                target: "aide_audit",
                "[CRITICAL_DRIFT] user={:?} field={} local={} chain={}",
                d.user,
                d.field,
                d.local,
                d.chain
            );
        }
        engine.pause_matching();
        return Ok(false);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::BalanceDrift;
    use alloy_primitives::address;

    #[test]
    fn threshold_zero_is_strict() {
        let report = ReconciliationReport {
            drifts: vec![BalanceDrift {
                user: address!("1111111111111111111111111111111111111111"),
                field: "aitBalance",
                local: U256::from(1u64),
                chain: U256::from(2u64),
            }],
        };
        assert!(report_exceeds_threshold(&report, U256::ZERO));
    }

    #[test]
    fn threshold_allows_small_diff() {
        let report = ReconciliationReport {
            drifts: vec![BalanceDrift {
                user: address!("1111111111111111111111111111111111111111"),
                field: "aitBalance",
                local: U256::from(100u64),
                chain: U256::from(101u64),
            }],
        };
        assert!(!report_exceeds_threshold(&report, U256::from(5u64)));
        assert!(report_exceeds_threshold(&report, U256::ZERO));
    }
}
