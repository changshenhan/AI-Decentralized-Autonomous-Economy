//! 全景遥测：为 Dashboard 提供 JSON 与 Prometheus 文本指标（AIDE_DASHBOARD_PROTOTYPE §1 / §2）
//!
//! 指标：`aide_vstk_volume_24h`、`aide_treasury_tax_yield`、`aide_guardrail_status`、`aide_velocity_v`、
//! `aide_system_health_score`（红绿灯）、`aide_ledger_alignment`、`aide_identity_model`（TEE 1.0）

use alloy_primitives::U256;
use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use parking_lot::RwLock;
use serde::Serialize;
use serde_json::Value;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;

/// 撮合暂停原因（与 `[CRITICAL_DRIFT]` / `[CRITICAL_SETTLEMENT_FAILURE]` 对齐）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchingPauseReason {
    Drift,
    SettlementFailure,
}

/// 24 小时滑动窗口（与 `forecaster::VelocityWindow` 行为一致）
#[derive(Debug)]
struct VolumeWindow24h {
    window: Duration,
    samples: VecDeque<(Instant, U256)>,
    sum: U256,
}

impl VolumeWindow24h {
    fn new(window: Duration) -> Self {
        Self {
            window,
            samples: VecDeque::new(),
            sum: U256::ZERO,
        }
    }

    fn record(&mut self, amount: U256) {
        let now = Instant::now();
        self.samples.push_back((now, amount));
        self.sum = self.sum.saturating_add(amount);
        self.evict_old(now);
    }

    fn evict_old(&mut self, now: Instant) {
        while let Some(&(t, a)) = self.samples.front() {
            if now.duration_since(t) > self.window {
                self.samples.pop_front();
                self.sum = self.sum.saturating_sub(a);
            } else {
                break;
            }
        }
    }

    fn total(&mut self) -> U256 {
        self.evict_old(Instant::now());
        self.sum
    }
}

/// 最近至多 1000 个**区块高度**上的 AIT 名义流量（用于 `aide_velocity_v`）
#[derive(Debug)]
struct Last1000BlocksAit {
    /// 按区块高度合并后的 (block, sum_ait)
    blocks: VecDeque<(u64, U256)>,
    max_blocks: usize,
}

impl Last1000BlocksAit {
    fn new() -> Self {
        Self {
            blocks: VecDeque::new(),
            max_blocks: 1000,
        }
    }

    fn record(&mut self, block_number: u64, ait_notional: U256) {
        if let Some((b, sum)) = self.blocks.back_mut() {
            if *b == block_number {
                *sum = sum.saturating_add(ait_notional);
                return;
            }
        }
        self.blocks.push_back((block_number, ait_notional));
        while self.blocks.len() > self.max_blocks {
            self.blocks.pop_front();
        }
    }

    /// 近窗口内 AIT 总量 / 区块数（无块时为 0）
    fn velocity_per_block_avg(&self) -> U256 {
        let n = self.blocks.len() as u64;
        if n == 0 {
            return U256::ZERO;
        }
        let total: U256 = self
            .blocks
            .iter()
            .fold(U256::ZERO, |acc, (_, v)| acc.saturating_add(*v));
        total / U256::from(n)
    }
}

#[derive(Debug)]
struct Inner {
    vstk_window: VolumeWindow24h,
    tax_window: VolumeWindow24h,
    /// 近 24h 经 `record_trade_settled` 累计的 AIT 名义成交额（wei），供 Dashboard `V` 与 `/summary`
    ait_notional_24h: VolumeWindow24h,
    treasury_balance_hint: U256,
    guardrail_paused: bool,
    block_ait: Last1000BlocksAit,
    /// 链上 `InternalMarket` 监听器是否仍存活（退出则视为断开 → 健康分 0）
    chain_listener_connected: bool,
    /// Sync-Audit 是否与链一致（与影子账本对齐度）
    ledger_alignment_ok: bool,
    /// 撮合因漂移或结算失败暂停
    matching_pause: Option<MatchingPauseReason>,
    address_book: ContractAddressBook,
}

/// 核心合约地址（由 `aide_daemon` 从环境变量或 `AIDE_DEPLOYMENT_JSON` 注入）
#[derive(Clone, Debug, Default, Serialize)]
pub struct ContractAddressBook {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aitoken: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub treasury: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guardrail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal_market: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_market: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staking_pool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_identity_registry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dual_pool_vault: Option<String>,
}

/// `GET /summary` 聚合视图（Dashboard 头部一块 JSON）
#[derive(Serialize)]
pub struct DashboardSummaryJson {
    pub aide_system_health_score: u8,
    pub aide_ledger_alignment: String,
    pub aide_identity_model: String,
    /// `Active`：熔断未暂停；`Paused`：`Guardrail.paused`
    pub circuit_breaker: String,
    /// 近 24h AIT 名义成交额（wei 字符串）
    pub ait_notional_volume_24h_wei: String,
    /// 引擎侧近 ≤1000 块平均每块 AIT 名义（与 `aide_velocity_v` 一致）
    pub ait_velocity_per_block_wei: String,
    pub aide_matching_pause_reason: Option<String>,
    pub contracts: ContractAddressBook,
}

/// 可共享的遥测状态；由 `aide_daemon` 或测试在成交/抽税/链上读回后更新。
#[derive(Clone)]
pub struct TelemetryHub {
    inner: Arc<RwLock<Inner>>,
}

impl TelemetryHub {
    pub fn new() -> Self {
        let day = Duration::from_secs(24 * 3600);
        Self {
            inner: Arc::new(RwLock::new(Inner {
                vstk_window: VolumeWindow24h::new(day),
                tax_window: VolumeWindow24h::new(day),
                ait_notional_24h: VolumeWindow24h::new(day),
                treasury_balance_hint: U256::ZERO,
                guardrail_paused: false,
                block_ait: Last1000BlocksAit::new(),
                chain_listener_connected: true,
                ledger_alignment_ok: true,
                matching_pause: None,
                address_book: ContractAddressBook::default(),
            })),
        }
    }

    /// 注入核心合约地址（环境变量或部署 JSON，见 `load_contract_address_book_from_env`）
    pub fn set_contract_addresses(&self, book: ContractAddressBook) {
        self.inner.write().address_book = book;
    }

    pub fn contract_addresses_snapshot(&self) -> ContractAddressBook {
        self.inner.read().address_book.clone()
    }

    /// 链上事件监听任务结束或 RPC 不可达时置 `false`（健康分 0）
    pub fn set_chain_listener_connected(&self, connected: bool) {
        self.inner.write().chain_listener_connected = connected;
    }

    /// Sync-Audit：`true` 影子与链一致；`false` 曾检测到漂移（直至下一轮成功对账）
    pub fn set_ledger_alignment_ok(&self, ok: bool) {
        self.inner.write().ledger_alignment_ok = ok;
    }

    /// `[CRITICAL_DRIFT]` / `[CRITICAL_SETTLEMENT_FAILURE]` 时设置；恢复撮合后由运维路径清除
    pub fn set_matching_pause(&self, reason: Option<MatchingPauseReason>) {
        self.inner.write().matching_pause = reason;
    }

    fn compute_health_score(g: &Inner) -> u8 {
        if !g.chain_listener_connected {
            return 0;
        }
        if g.matching_pause.is_some() {
            return 50;
        }
        100
    }

    /// vSTK 成交名义侧：记入 24h 窗口（vstk_amount * price 或引擎侧约定的名义额）
    pub fn record_vstk_notional(&self, amount: U256) {
        self.inner.write().vstk_window.record(amount);
    }

    /// 国库税收：记入 24h 窗口（如 `ProtocolTaxCollected` 的 fee）
    pub fn record_tax(&self, fee: U256) {
        self.inner.write().tax_window.record(fee);
    }

    pub fn set_treasury_balance_hint(&self, bal: U256) {
        self.inner.write().treasury_balance_hint = bal;
    }

    pub fn set_guardrail_paused(&self, paused: bool) {
        self.inner.write().guardrail_paused = paused;
    }

    /// 撮合结算回调：`block_number` 为链上块高，`ait_notional` 为该笔 AIT 名义
    pub fn record_trade_settled(&self, block_number: u64, ait_notional: U256, vstk_notional: U256) {
        let mut g = self.inner.write();
        g.block_ait.record(block_number, ait_notional);
        g.ait_notional_24h.record(ait_notional);
        g.vstk_window.record(vstk_notional);
    }

    /// 导出当前指标（供测试或 JSON-RPC 封装）
    pub fn to_metrics_json(&self) -> MetricsJson {
        let mut g = self.inner.write();
        let v24 = g.vstk_window.total();
        let tax24 = g.tax_window.total();
        let bal = g.treasury_balance_hint;
        let yield_ratio = treasury_tax_yield_ratio(tax24, bal);
        let vel = g.block_ait.velocity_per_block_avg();
        let guard = if g.guardrail_paused { 1u8 } else { 0u8 };
        let health = Self::compute_health_score(&g);
        let ledger = if g.ledger_alignment_ok {
            "ok"
        } else {
            "drift"
        }
        .to_string();
        let pause_reason = g.matching_pause.map(|r| match r {
            MatchingPauseReason::Drift => "drift".to_string(),
            MatchingPauseReason::SettlementFailure => "settlement_failure".to_string(),
        });
        MetricsJson {
            aide_vstk_volume_24h: v24.to_string(),
            aide_treasury_tax_yield: yield_ratio,
            aide_guardrail_status: guard,
            aide_velocity_v: vel.to_string(),
            aide_system_health_score: health,
            aide_ledger_alignment: ledger,
            aide_identity_model: "TEE_ECDSA_1.0".to_string(),
            aide_matching_pause_reason: pause_reason,
        }
    }

    /// Dashboard 单端点聚合（健康分、合约地址、`V`、熔断语义）
    pub fn to_summary_json(&self) -> DashboardSummaryJson {
        let mut g = self.inner.write();
        let ait24 = g.ait_notional_24h.total();
        let vel = g.block_ait.velocity_per_block_avg();
        let guard = g.guardrail_paused;
        let health = Self::compute_health_score(&g);
        let ledger = if g.ledger_alignment_ok {
            "ok"
        } else {
            "drift"
        }
        .to_string();
        let pause_reason = g.matching_pause.map(|r| match r {
            MatchingPauseReason::Drift => "drift".to_string(),
            MatchingPauseReason::SettlementFailure => "settlement_failure".to_string(),
        });
        let book = g.address_book.clone();
        DashboardSummaryJson {
            aide_system_health_score: health,
            aide_ledger_alignment: ledger,
            aide_identity_model: "TEE_ECDSA_1.0".to_string(),
            circuit_breaker: if guard {
                "Paused".to_string()
            } else {
                "Active".to_string()
            },
            ait_notional_volume_24h_wei: ait24.to_string(),
            ait_velocity_per_block_wei: vel.to_string(),
            aide_matching_pause_reason: pause_reason,
            contracts: book,
        }
    }

    fn snapshot_prometheus(&self) -> String {
        let j = self.to_metrics_json();
        format!(
            "# HELP aide_vstk_volume_24h Notional vSTK-related volume over 24h (wei).\n\
             # TYPE aide_vstk_volume_24h gauge\n\
             aide_vstk_volume_24h {}\n\
             # HELP aide_treasury_tax_yield Approximate tax collected / treasury balance (24h).\n\
             # TYPE aide_treasury_tax_yield gauge\n\
             aide_treasury_tax_yield {:.6}\n\
             # HELP aide_guardrail_status 1=paused 0=normal.\n\
             # TYPE aide_guardrail_status gauge\n\
             aide_guardrail_status {}\n\
             # HELP aide_velocity_v Average AIT notional per block over last <=1000 blocks (wei).\n\
             # TYPE aide_velocity_v gauge\n\
             aide_velocity_v {}\n\
             # HELP aide_system_health_score 100=ok 50=matching_paused 0=chain_listener_down.\n\
             # TYPE aide_system_health_score gauge\n\
             aide_system_health_score {}\n",
            j.aide_vstk_volume_24h,
            j.aide_treasury_tax_yield,
            j.aide_guardrail_status,
            j.aide_velocity_v,
            j.aide_system_health_score
        )
    }
}

#[derive(Serialize)]
pub struct MetricsJson {
    pub aide_vstk_volume_24h: String,
    pub aide_treasury_tax_yield: f64,
    pub aide_guardrail_status: u8,
    pub aide_velocity_v: String,
    /// 100 正常；50 撮合因漂移/结算失败暂停；0 链监听断开
    pub aide_system_health_score: u8,
    /// `ok` | `drift`（影子与链不一致时的最近一次 Sync-Audit 结果）
    pub aide_ledger_alignment: String,
    /// AIDE 1.0 固定为 TEE ECDSA 身份路径
    pub aide_identity_model: String,
    pub aide_matching_pause_reason: Option<String>,
}

fn treasury_tax_yield_ratio(tax_24h: U256, treasury_bal: U256) -> f64 {
    if treasury_bal.is_zero() {
        return 0.0;
    }
    let t = u256_to_f64_approx(tax_24h);
    let b = u256_to_f64_approx(treasury_bal);
    if b <= 0.0 {
        return 0.0;
    }
    (t / b).min(1e9)
}

fn u256_to_f64_approx(x: U256) -> f64 {
    // 足够 Dashboard 展示；极大值会丢失精度
    let s = x.to_string();
    s.parse::<f64>().unwrap_or(0.0)
}

async fn route_json(State(hub): State<TelemetryHub>) -> Json<MetricsJson> {
    Json(hub.to_metrics_json())
}

async fn route_metrics(State(hub): State<TelemetryHub>) -> (StatusCode, String) {
    (StatusCode::OK, hub.snapshot_prometheus())
}

async fn route_summary(State(hub): State<TelemetryHub>) -> Json<DashboardSummaryJson> {
    Json(hub.to_summary_json())
}

fn json_addr(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 从 `AIDE_DEPLOYMENT_JSON` 或常见 `deployments/*.json` 与环境变量（`INTERNAL_MARKET_ADDRESS`、`AIDE_*` 等）拼装地址簿；后者覆盖前者。
pub fn load_contract_address_book_from_env() -> ContractAddressBook {
    let mut b = ContractAddressBook::default();

    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(p) = std::env::var("AIDE_DEPLOYMENT_JSON") {
        candidates.push(Path::new(&p).to_path_buf());
    }
    candidates.push(Path::new("deployments/localhost.json").to_path_buf());
    candidates.push(Path::new("deployments/base_sepolia.json").to_path_buf());
    candidates.push(Path::new("deployments/base.json").to_path_buf());

    for p in candidates {
        if p.exists() {
            if let Ok(text) = std::fs::read_to_string(&p) {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    b.aitoken = json_addr(&v, "AIToken");
                    b.treasury = json_addr(&v, "Treasury");
                    b.guardrail = json_addr(&v, "Guardrail");
                    b.internal_market = json_addr(&v, "InternalMarket");
                    b.task_market = json_addr(&v, "TaskMarket");
                    b.staking_pool = json_addr(&v, "StakingPool");
                    b.ai_identity_registry = json_addr(&v, "AIIdentityRegistry");
                    b.dual_pool_vault = json_addr(&v, "DualPoolVault");
                    break;
                }
            }
        }
    }

    let overlay = |dst: &mut Option<String>, keys: &[&str]| {
        for key in keys {
            if let Ok(v) = std::env::var(key) {
                let t = v.trim();
                if !t.is_empty() {
                    *dst = Some(t.to_string());
                    return;
                }
            }
        }
    };

    overlay(&mut b.aitoken, &["AIDE_AITOKEN"]);
    overlay(&mut b.treasury, &["AIDE_TREASURY", "TREASURY_ADDRESS"]);
    overlay(&mut b.guardrail, &["AIDE_GUARDRAIL", "GUARDRAIL_ADDRESS"]);
    overlay(
        &mut b.internal_market,
        &["AIDE_INTERNAL_MARKET", "INTERNAL_MARKET_ADDRESS"],
    );
    overlay(&mut b.task_market, &["AIDE_TASK_MARKET"]);
    overlay(&mut b.staking_pool, &["AIDE_STAKING_POOL"]);
    overlay(
        &mut b.ai_identity_registry,
        &["AIDE_REGISTRY", "AIDE_AI_IDENTITY_REGISTRY"],
    );
    overlay(&mut b.dual_pool_vault, &["AIDE_DUAL_POOL_VAULT"]);

    b
}

/// 与 `AIDE_DASHBOARD_PROTOTYPE` 一致：`GET /` 或 `GET /metrics` JSON；`GET /metrics/prometheus` 为文本；`GET /summary` 为 Dashboard 聚合头
pub fn router(hub: TelemetryHub) -> Router {
    Router::new()
        .route("/", get(route_json))
        .route("/metrics", get(route_json))
        .route("/summary", get(route_summary))
        .route("/metrics/prometheus", get(route_metrics))
        .with_state(hub)
}

/// 阻塞当前任务在 `addr` 上提供 HTTP 服务（需在 `#[tokio::main]` 下与 `spawn` 配合使用）
pub async fn serve(hub: TelemetryHub, addr: SocketAddr) -> Result<(), std::io::Error> {
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, router(hub)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_roundtrip() {
        let h = TelemetryHub::new();
        h.set_treasury_balance_hint(U256::from(10u128.pow(18)));
        h.record_tax(U256::from(10u128.pow(16)));
        h.record_trade_settled(100, U256::from(5u64), U256::from(3u64));
        let j = h.to_metrics_json();
        assert!(j.aide_treasury_tax_yield > 0.0);
        assert_eq!(j.aide_guardrail_status, 0);
        assert_eq!(j.aide_system_health_score, 100);
        assert_eq!(j.aide_ledger_alignment, "ok");
    }

    #[test]
    fn health_score_50_when_matching_paused() {
        let h = TelemetryHub::new();
        h.set_matching_pause(Some(MatchingPauseReason::Drift));
        assert_eq!(h.to_metrics_json().aide_system_health_score, 50);
        h.set_matching_pause(Some(MatchingPauseReason::SettlementFailure));
        assert_eq!(h.to_metrics_json().aide_system_health_score, 50);
    }

    #[test]
    fn health_score_0_when_chain_listener_down() {
        let h = TelemetryHub::new();
        h.set_chain_listener_connected(false);
        assert_eq!(h.to_metrics_json().aide_system_health_score, 0);
    }

    #[test]
    fn summary_json_matches_guardrail_and_health() {
        let h = TelemetryHub::new();
        h.set_guardrail_paused(true);
        let s = h.to_summary_json();
        assert_eq!(s.circuit_breaker, "Paused");
        assert_eq!(s.aide_system_health_score, 100);
        h.set_chain_listener_connected(false);
        assert_eq!(h.to_summary_json().aide_system_health_score, 0);
    }
}
