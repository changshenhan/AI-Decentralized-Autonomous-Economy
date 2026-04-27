//! 链上状态与内存簿的最终一致性：快照 + 对账报告（ENGINE_SYNC_SPEC §1、§4）
//!
//! 生产环境应周期性拉取 `InternalMarket.aitBalance` / `vstkBalance` / `orders(orderId)` 与引擎本地模型比对。
//!
//! **税率与链上对齐**：`Treasury.getCurrentTaxRate()` 不在本模块存储；由 `audit::run_sync_audit_once` 在同一 RPC
//! 会话中拉取并写入 [`crate::matching::MatchEngine::set_tax_rate_bps`]，供结算路径税费估算与遥测与合约一致。

use alloy_primitives::{Address, U256};
use dashmap::DashMap;
use std::sync::Arc;

/// 链上只读快照（由 JSON-RPC `eth_call` 批量填充）
#[derive(Clone, Debug, Default)]
pub struct ChainBalanceSnapshot {
    /// user -> ait 内部余额
    pub ait: DashMap<Address, U256>,
    /// (company_id, user) -> vSTK
    pub vstk: DashMap<(u64, Address), U256>,
}

impl ChainBalanceSnapshot {
    /// 创建空快照
    pub fn new() -> Self {
        Self {
            ait: DashMap::new(),
            vstk: DashMap::new(),
        }
    }
}

/// 对账差异
#[derive(Clone, Debug)]
pub struct BalanceDrift {
    pub user: Address,
    pub field: &'static str,
    pub local: U256,
    pub chain: U256,
}

/// 对账报告
#[derive(Clone, Debug, Default)]
pub struct ReconciliationReport {
    pub drifts: Vec<BalanceDrift>,
}

/// 协调器：持有本地影子余额（可由 `TradeSettled` / 成交回填更新），与链上快照 diff
#[derive(Clone)]
pub struct SyncCoordinator {
    local_ait: Arc<DashMap<Address, U256>>,
    local_vstk: Arc<DashMap<(u64, Address), U256>>,
}

impl Default for SyncCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncCoordinator {
    /// 新建协调器
    pub fn new() -> Self {
        Self {
            local_ait: Arc::new(DashMap::new()),
            local_vstk: Arc::new(DashMap::new()),
        }
    }

    /// 模拟成交后更新本地影子（真实系统应在 `TradeSettled` 确认后调用）
    pub fn apply_fill_shadow(
        &self,
        company_id: u64,
        buyer: Address,
        seller: Address,
        vstk: U256,
        ait: U256,
    ) {
        {
            let mut e = self.local_ait.entry(buyer).or_insert(U256::ZERO);
            *e = e.saturating_sub(ait);
        }
        {
            let mut e = self.local_ait.entry(seller).or_insert(U256::ZERO);
            *e = e.saturating_add(ait);
        }
        {
            let mut e = self
                .local_vstk
                .entry((company_id, seller))
                .or_insert(U256::ZERO);
            *e = e.saturating_sub(vstk);
        }
        {
            let mut e = self
                .local_vstk
                .entry((company_id, buyer))
                .or_insert(U256::ZERO);
            *e = e.saturating_add(vstk);
        }
    }

    /// 影子 AIT 侧用户地址（供 Sync-Audit RPC 批量拉取 `aitBalance`）
    pub fn shadow_ait_users(&self) -> Vec<Address> {
        self.local_ait.iter().map(|r| *r.key()).collect()
    }

    /// 影子 vSTK 键（供拉取 `vstkBalance(companyId, user)`）
    pub fn shadow_vstk_keys(&self) -> Vec<(u64, Address)> {
        self.local_vstk.iter().map(|r| *r.key()).collect()
    }

    /// 与链上快照对账
    pub fn reconcile(&self, chain: &ChainBalanceSnapshot) -> ReconciliationReport {
        let mut drifts = Vec::new();
        for r in self.local_ait.iter() {
            let addr = *r.key();
            let local = *r.value();
            let c = chain.ait.get(&addr).map(|x| *x).unwrap_or(U256::ZERO);
            if local != c {
                drifts.push(BalanceDrift {
                    user: addr,
                    field: "aitBalance",
                    local,
                    chain: c,
                });
            }
        }
        for r in self.local_vstk.iter() {
            let k = *r.key();
            let local = *r.value();
            let c = chain.vstk.get(&k).map(|x| *x).unwrap_or(U256::ZERO);
            if local != c {
                drifts.push(BalanceDrift {
                    user: k.1,
                    field: "vstkBalance",
                    local,
                    chain: c,
                });
            }
        }
        ReconciliationReport { drifts }
    }
}
