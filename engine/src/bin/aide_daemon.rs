//! AIDE 守护进程（Engine Daemon）
//!
//! - EventStreamer：链上事件流（带 Reorg-Safe BlockBuffer）
//! - MatchRunner：撮合引擎（MatchEngine）+ EconomicForecaster
//! - SettlerQueue：批量结算 + 指数退避重试
//! - Sync-Audit：每 N 个区块对账，异常时暂停撮合

use aide_engine::{
    load_contract_address_book_from_env, load_settler_signer, run_internal_market_listener,
    run_sync_audit_once, send_settle_trade_with_retry, telemetry_serve, ChainListenerConfig, Fill,
    MarketChainEvent, MatchEngine, MatchingPauseReason, NotificationManager, OrderRecord, Side,
    SyncCoordinator, TelemetryHub,
};
use alloy_primitives::{address, Address, B256, U256};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    Arc,
};
use tokio::sync::mpsc;
use tokio::time::{self, Duration, Instant};
use tracing::{error, info, warn};

/// 事件枚举（实际监听时应从 `chain.rs` 解码而来）
#[derive(Debug, Clone)]
enum EngineEvent {
    OrderPlaced {
        block: u64,
        order_id: u64,
        company_id: u64,
        maker: Address,
        side: Side,
        price_ray: U256,
        amount: U256,
        timestamp: u64,
    },
    OrderCancelled {
        block: u64,
        company_id: u64,
        order_id: u64,
    },
    TradeSettled {
        block: u64,
        company_id: u64,
        buyer: Address,
        seller: Address,
        vstk_amount: U256,
        ait_notional: U256,
    },
    NewBlock {
        number: u64,
    },
}

/// Reorg-Safe 缓冲区：仅当 `block >= ev.block + confirmations` 时才向撮合层释放
struct BlockBuffer {
    confirmations: u64,
    /// (block, event)
    queue: VecDeque<(u64, EngineEvent)>,
}

impl BlockBuffer {
    fn new(confirmations: u64) -> Self {
        Self {
            confirmations,
            queue: VecDeque::new(),
        }
    }

    fn push(&mut self, block: u64, ev: EngineEvent) {
        self.queue.push_back((block, ev));
    }

    /// 根据最新区块高度，提取已确认事件
    fn drain_confirmed(&mut self, head: u64, out: &mut Vec<EngineEvent>) {
        while let Some(&(b, _)) = self.queue.front() {
            if head >= b + self.confirmations {
                if let Some((_, ev)) = self.queue.pop_front() {
                    out.push(ev);
                }
            } else {
                break;
            }
        }
    }
}

#[derive(Clone)]
struct SharedState {
    engine: MatchEngine,
    sync: SyncCoordinator,
    match_paused: Arc<AtomicBool>,
    /// 最近 1 秒内处理的订单数（用于 TPS）
    order_counter: Arc<AtomicU64>,
    /// 当前待结算数量
    pending_settlements: Arc<AtomicU64>,
    /// `settleTrade` 连续失败次数（达 3 触发 `[CRITICAL_SETTLEMENT_FAILURE]`）
    settlement_fail_streak: Arc<AtomicU32>,
    /// 经济速率（窗口内累计名义 AIT，简化指标）
    economic_velocity_wei: Arc<AtomicU64>,
    /// 链尖高度（用于遥测 `aide_velocity_v` 的块锚）
    last_block_height: Arc<AtomicU64>,
    /// 全景遥测（`/metrics` JSON + Prometheus）
    telemetry: TelemetryHub,
    notifier: NotificationManager,
}

fn aide_production_mode() -> bool {
    std::env::var("AIDE_PRODUCTION_MODE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// `AIDE_PRODUCTION_MODE=1`：强制链上监听、关键环境变量齐全，否则 panic。
fn assert_aide_production_preflight() {
    if !aide_production_mode() {
        return;
    }
    let rpc = std::env::var("ETH_RPC_URL").unwrap_or_default();
    if rpc.trim().is_empty() {
        panic!("AIDE_PRODUCTION_MODE=1 requires ETH_RPC_URL");
    }
    for key in [
        "INTERNAL_MARKET_ADDRESS",
        "TREASURY_ADDRESS",
        "GUARDRAIL_ADDRESS",
    ] {
        let v = std::env::var(key).unwrap_or_default();
        if v.trim().is_empty() {
            panic!("AIDE_PRODUCTION_MODE=1 requires {}", key);
        }
        if v.parse::<Address>().is_err() {
            panic!("AIDE_PRODUCTION_MODE=1: {} is not a valid address", key);
        }
    }
    let sk = std::env::var("SETTLER_PRIVATE_KEY").unwrap_or_default();
    if sk.trim().is_empty() {
        panic!("AIDE_PRODUCTION_MODE=1 requires SETTLER_PRIVATE_KEY");
    }
    std::env::set_var("AIDE_USE_CHAIN_LISTENER", "1");
    info!("AIDE_PRODUCTION_MODE: preflight OK (chain listener forced on)");
}

/// 生产模式下若未配置 `ALERT_WEBHOOK_URL`，高亮提示运维（手机/IM 不会收到 CRITICAL 推送）。
fn warn_if_no_alert_webhook_in_production() {
    if !aide_production_mode() {
        return;
    }
    let url = std::env::var("ALERT_WEBHOOK_URL").unwrap_or_default();
    if url.trim().is_empty() {
        eprintln!(
            "\n\x1b[33m\x1b[1m[WARNING]\x1b[0m \x1b[1m系统运行在无告警监控状态\x1b[0m: 未设置 \x1b[36mALERT_WEBHOOK_URL\x1b[0m。\n\
             \x1b[33m         CRITICAL 事件仅写入日志，不会推送到 Discord / Slack / Telegram。\x1b[0m\n"
        );
        warn!(target: "aide_ops", "系统运行在无告警监控状态: ALERT_WEBHOOK_URL 未配置");
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,aide_engine=info".to_string()),
        )
        .init();

    info!("Starting AIDE Engine Daemon");

    assert_aide_production_preflight();
    warn_if_no_alert_webhook_in_production();

    let telemetry = TelemetryHub::new();
    telemetry.set_contract_addresses(load_contract_address_book_from_env());
    let notifier = NotificationManager::from_env();
    let telemetry_http = telemetry.clone();

    let bind: SocketAddr = std::env::var("TELEMETRY_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:9898".to_string())
        .parse()
        .expect("TELEMETRY_ADDR invalid");
    tokio::spawn(async move {
        info!(
            ?bind,
            "telemetry HTTP (GET /summary, /metrics, /metrics/prometheus)"
        );
        if let Err(e) = telemetry_serve(telemetry_http, bind).await {
            error!(?e, "telemetry server stopped");
        }
    });

    if let (Ok(rpc), Ok(gr)) = (
        std::env::var("ETH_RPC_URL"),
        std::env::var("GUARDRAIL_ADDRESS"),
    ) {
        let tel = telemetry.clone();
        tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("reqwest");
            loop {
                match fetch_guardrail_paused(&client, &rpc, &gr).await {
                    Ok(p) => tel.set_guardrail_paused(p),
                    Err(e) => warn!(?e, "guardrail eth_call poll"),
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    }

    let state = SharedState {
        engine: MatchEngine::new(),
        sync: SyncCoordinator::new(),
        match_paused: Arc::new(AtomicBool::new(false)),
        order_counter: Arc::new(AtomicU64::new(0)),
        pending_settlements: Arc::new(AtomicU64::new(0)),
        settlement_fail_streak: Arc::new(AtomicU32::new(0)),
        economic_velocity_wei: Arc::new(AtomicU64::new(0)),
        last_block_height: Arc::new(AtomicU64::new(0)),
        telemetry,
        notifier,
    };

    // Event pipeline：EventStreamer -> MatchRunner -> SettlerQueue
    let (event_tx, event_rx) = mpsc::channel::<EngineEvent>(4096);
    let (fill_tx, fill_rx) = mpsc::channel::<Fill>(8192);

    let state_es = state.clone();
    let state_mr = state.clone();
    let state_sq = state.clone();
    let state_metrics = state.clone();
    let state_audit = state.clone();

    tokio::spawn(async move {
        run_sync_audit_loop(state_audit).await;
    });

    // EventStreamer（此处为占位模拟；生产应对接 chain listener + BlockBuffer）
    let event_task = tokio::spawn(async move {
        run_event_streamer(state_es, event_tx).await;
    });

    // MatchRunner：消费确认后的挂单 -> 撮合 -> Fills
    let match_task = tokio::spawn(async move {
        run_match_runner(state_mr, event_rx, fill_tx).await;
    });

    // SettlerQueue：带重试的结算队列
    let settler_task = tokio::spawn(async move {
        run_settler_queue(state_sq, fill_rx).await;
    });

    // Metrics & 健康日志
    let metrics_task = tokio::spawn(async move {
        run_metrics_logger(state_metrics).await;
    });

    let _ = tokio::join!(event_task, match_task, settler_task, metrics_task);
}

/// 每 `AIDE_SYNC_AUDIT_INTERVAL_BLOCKS`（默认 50）拉取 `InternalMarket` / `Treasury` 与影子对账；漂移超阈值则 `[CRITICAL_DRIFT]` 并 `pause_matching`
async fn run_sync_audit_loop(state: SharedState) {
    let mut last_audited_block: u64 = 0;
    let interval_blocks: u64 = std::env::var("AIDE_SYNC_AUDIT_INTERVAL_BLOCKS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
    let threshold: U256 = std::env::var("AIDE_SYNC_AUDIT_DRIFT_THRESHOLD_WEI")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(U256::ZERO);

    let rpc = match std::env::var("ETH_RPC_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => {
            info!("Sync-Audit: ETH_RPC_URL unset — periodic on-chain audit disabled");
            return;
        }
    };
    let market = match std::env::var("INTERNAL_MARKET_ADDRESS")
        .ok()
        .and_then(|s| s.parse::<Address>().ok())
    {
        Some(a) => a,
        _ => {
            info!("Sync-Audit: INTERNAL_MARKET_ADDRESS unset — periodic audit disabled");
            return;
        }
    };
    let treasury = match std::env::var("TREASURY_ADDRESS")
        .ok()
        .and_then(|s| s.parse::<Address>().ok())
    {
        Some(a) => a,
        _ => {
            info!("Sync-Audit: TREASURY_ADDRESS unset — periodic audit disabled");
            return;
        }
    };

    let mut tick = time::interval(Duration::from_secs(2));
    loop {
        tick.tick().await;
        let head = state.last_block_height.load(Ordering::Relaxed);
        if head == 0 || head < interval_blocks {
            continue;
        }
        if head % interval_blocks != 0 {
            continue;
        }
        if head == last_audited_block {
            continue;
        }
        last_audited_block = head;

        match run_sync_audit_once(
            &rpc,
            market,
            treasury,
            &state.sync,
            &state.engine,
            threshold,
        )
        .await
        {
            Ok(true) => {
                state.telemetry.set_ledger_alignment_ok(true);
                info!(target: "aide_audit", block = head, "Sync-Audit: Shadow State Match");
            }
            Ok(false) => {
                state.telemetry.set_ledger_alignment_ok(false);
                state
                    .telemetry
                    .set_matching_pause(Some(MatchingPauseReason::Drift));
                error!(target: "aide_audit", "[CRITICAL_DRIFT] matching halted; fix drift then resume");
                state.match_paused.store(true, Ordering::SeqCst);
                state.notifier.send_critical_alert(
                    "CRITICAL_DRIFT",
                    format!(
                        "Sync-Audit: shadow state diverged from chain. internal_market={:#x} treasury={:#x}. See aide_audit logs.",
                        market, treasury
                    ),
                    state.telemetry.contract_addresses_snapshot(),
                );
            }
            Err(e) => {
                warn!(target: "aide_audit", ?e, "Sync-Audit RPC failed (no pause)");
            }
        }
    }
}

/// EventStreamer：可选真实链监听（`AIDE_USE_CHAIN_LISTENER=1`）或本地 tick 模拟。`AIDE_PRODUCTION_MODE=1` 时强制链上监听。
async fn run_event_streamer(state: SharedState, tx: mpsc::Sender<EngineEvent>) {
    let use_chain = aide_production_mode()
        || std::env::var("AIDE_USE_CHAIN_LISTENER")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

    if use_chain {
        let Some(market) = std::env::var("INTERNAL_MARKET_ADDRESS")
            .ok()
            .and_then(|s| s.parse::<Address>().ok())
        else {
            error!("AIDE_USE_CHAIN_LISTENER set but INTERNAL_MARKET_ADDRESS missing/invalid");
            return;
        };
        let cfg = ChainListenerConfig {
            market,
            rpc_url_http: std::env::var("ETH_RPC_URL").ok(),
            ws_url: std::env::var("ETH_WS_URL").ok(),
            from_block: std::env::var("AIDE_FROM_BLOCK")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
        };
        let (chain_tx, mut chain_rx) = mpsc::channel::<MarketChainEvent>(8192);
        let tel_listener = state.telemetry.clone();
        tokio::spawn(async move {
            if let Err(e) = run_internal_market_listener(cfg, chain_tx).await {
                error!(?e, "chain listener exited");
                tel_listener.set_chain_listener_connected(false);
            }
        });

        let mut buf = BlockBuffer::new(3);
        let mut order_company: HashMap<u64, u64> = HashMap::new();

        while let Some(mev) = chain_rx.recv().await {
            match mev {
                MarketChainEvent::NewHead { number } => {
                    state.last_block_height.store(number, Ordering::Relaxed);
                    let mut confirmed = Vec::new();
                    buf.drain_confirmed(number, &mut confirmed);
                    for ev in confirmed {
                        if tx.send(ev).await.is_err() {
                            return;
                        }
                    }
                    if tx.send(EngineEvent::NewBlock { number }).await.is_err() {
                        return;
                    }
                }
                MarketChainEvent::OrderPlaced {
                    block_number,
                    order_id,
                    company_id,
                    maker,
                    side,
                    price_ray,
                    amount,
                    timestamp,
                    ..
                } => {
                    order_company.insert(order_id, company_id);
                    buf.push(
                        block_number,
                        EngineEvent::OrderPlaced {
                            block: block_number,
                            order_id,
                            company_id,
                            maker,
                            side: Side::from(side),
                            price_ray,
                            amount,
                            timestamp,
                        },
                    );
                }
                MarketChainEvent::TradeSettled {
                    block_number,
                    company_id,
                    buyer,
                    seller,
                    vstk_amount,
                    ait_notional,
                    ..
                } => {
                    buf.push(
                        block_number,
                        EngineEvent::TradeSettled {
                            block: block_number,
                            company_id,
                            buyer,
                            seller,
                            vstk_amount,
                            ait_notional,
                        },
                    );
                }
                MarketChainEvent::OrderCancelled {
                    block_number,
                    order_id,
                    maker: _,
                } => {
                    let company_id = order_company.remove(&order_id).unwrap_or(0);
                    if company_id == 0 {
                        warn!(
                            ?order_id,
                            "OrderCancelled: unknown order (no prior OrderPlaced in session)"
                        );
                        continue;
                    }
                    buf.push(
                        block_number,
                        EngineEvent::OrderCancelled {
                            block: block_number,
                            company_id,
                            order_id,
                        },
                    );
                }
            }
        }
        return;
    }

    // —— 本地模拟 ——
    let mut buf = BlockBuffer::new(3);
    let mut head: u64 = 0;
    let mut interval = time::interval(Duration::from_millis(200));

    loop {
        interval.tick().await;
        head += 1;
        state.last_block_height.store(head, Ordering::Relaxed);
        if tx
            .send(EngineEvent::NewBlock { number: head })
            .await
            .is_err()
        {
            break;
        }

        if head % 5 == 0 {
            let order_id = head;
            let ev = EngineEvent::OrderPlaced {
                block: head,
                order_id,
                company_id: 1,
                maker: address!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                side: Side::Buy,
                price_ray: U256::from(1_000_000_000_000_000_000u128),
                amount: U256::from(1000u64),
                timestamp: head,
            };
            buf.push(head, ev);
        }

        let mut confirmed = Vec::new();
        buf.drain_confirmed(head, &mut confirmed);
        for ev in confirmed {
            if tx.send(ev).await.is_err() {
                return;
            }
        }
    }
}

/// MatchRunner：将 `OrderPlaced` 转换为 `OrderRecord`，调用撮合引擎，并把 `Fill` 推送给 SettlerQueue。
async fn run_match_runner(
    state: SharedState,
    mut rx: mpsc::Receiver<EngineEvent>,
    tx_fill: mpsc::Sender<Fill>,
) {
    while let Some(ev) = rx.recv().await {
        match ev {
            EngineEvent::NewBlock { number } => {
                state.last_block_height.store(number, Ordering::Relaxed);
            }
            EngineEvent::OrderCancelled {
                company_id,
                order_id,
                ..
            } => {
                if let Err(e) = state.engine.ingest_cancel(company_id, order_id) {
                    warn!(?company_id, ?order_id, %e, "ingest_cancel failed");
                }
            }
            EngineEvent::TradeSettled {
                block,
                company_id,
                buyer,
                seller,
                vstk_amount,
                ait_notional,
            } => {
                state.last_block_height.store(block, Ordering::Relaxed);
                // 链上 TradeSettled 影子同步；与 Fill→settler 路径二选一计数，遥测主要在 flush_batch 落地
                state
                    .sync
                    .apply_fill_shadow(company_id, buyer, seller, vstk_amount, ait_notional);
                accumulate_velocity(&state, &ait_notional);
            }
            EngineEvent::OrderPlaced {
                order_id,
                company_id,
                maker,
                side,
                price_ray,
                amount,
                timestamp,
                ..
            } => {
                if state.match_paused.load(Ordering::Relaxed) || state.engine.is_matching_paused() {
                    warn!("match paused: dropping new order");
                    continue;
                }
                let rec = OrderRecord {
                    order_id,
                    company_id,
                    maker,
                    side,
                    price_ray,
                    remaining: amount,
                    timestamp,
                };
                match state.engine.limit_order_placement(rec) {
                    Ok(fills) => {
                        if !fills.is_empty() {
                            for f in fills {
                                accumulate_velocity(&state, &f.ait_notional);
                                state.pending_settlements.fetch_add(1, Ordering::Relaxed);
                                if tx_fill.send(f).await.is_err() {
                                    error!("settler queue closed");
                                    return;
                                }
                            }
                        }
                        state.order_counter.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        warn!(%e, "match engine error for order");
                    }
                }
            }
        }
    }
}

fn accumulate_velocity(state: &SharedState, delta: &U256) {
    // 简化：仅取低 64bit 参与指标，用于日志展示；真实经济模型应直接用 U256
    let low = delta.to::<u128>() as u64;
    state
        .economic_velocity_wei
        .fetch_add(low, Ordering::Relaxed);
}

/// SettlerQueue：消费 Fills，生成批量结算调用，并带指数退避进行重试（RPC 层由集成服务实现）。此处仅示意退避策略与计数。
async fn run_settler_queue(state: SharedState, mut rx: mpsc::Receiver<Fill>) {
    let mut batch: Vec<Fill> = Vec::with_capacity(1024);
    let mut last_flush = Instant::now();
    let flush_interval = Duration::from_millis(200);

    loop {
        tokio::select! {
            maybe = rx.recv() => {
                let Some(fill) = maybe else {
                    break;
                };
                batch.push(fill);
                if batch.len() >= 256 {
                    flush_batch(&state, &mut batch).await;
                    last_flush = Instant::now();
                }
            }
            _ = time::sleep_until(last_flush + flush_interval) => {
                if !batch.is_empty() {
                    flush_batch(&state, &mut batch).await;
                    last_flush = Instant::now();
                }
            }
        }
    }
}

async fn flush_batch(state: &SharedState, batch: &mut Vec<Fill>) {
    if batch.is_empty() {
        return;
    }
    let bn = state.last_block_height.load(Ordering::Relaxed);
    for f in batch.iter() {
        state
            .telemetry
            .record_trade_settled(bn, f.ait_notional, f.vstk_amount);
        state.telemetry.record_vstk_notional(f.vstk_amount);
        let tax_bps = state.engine.tax_rate_bps() as u128;
        if tax_bps > 0 && !f.ait_notional.is_zero() {
            let fee = f.ait_notional * U256::from(tax_bps) / U256::from(10_000u64);
            state.telemetry.record_tax(fee);
        }
    }

    let entropy = B256::from_slice(&[0u8; 32]);
    let fills = std::mem::take(batch);
    let calls = aide_engine::settler::batch_from_fills(&fills, entropy);

    let rpc = std::env::var("ETH_RPC_URL").ok();
    let market_addr = std::env::var("INTERNAL_MARKET_ADDRESS")
        .ok()
        .and_then(|s| s.parse::<Address>().ok());
    let signer_chain = load_settler_signer().ok();

    if let (Some(rpc_url), Some(market), Some((ref signer, chain_id))) =
        (rpc, market_addr, signer_chain)
    {
        for call in calls {
            match send_settle_trade_with_retry(&rpc_url, chain_id, market, signer, &call).await {
                Ok(h) => {
                    info!(?h, "settleTrade confirmed on-chain");
                    state.settlement_fail_streak.store(0, Ordering::SeqCst);
                    state.pending_settlements.fetch_sub(1, Ordering::Relaxed);
                }
                Err(e) => {
                    error!(?e, "settleTrade failed after retries");
                    let n = state.settlement_fail_streak.fetch_add(1, Ordering::SeqCst) + 1;
                    if n >= 3 {
                        error!(
                            target: "aide_settler",
                            "[CRITICAL_SETTLEMENT_FAILURE] settleTrade failed {} times consecutively; pausing matching",
                            n
                        );
                        state
                            .telemetry
                            .set_matching_pause(Some(MatchingPauseReason::SettlementFailure));
                        state.engine.pause_matching();
                        state.match_paused.store(true, Ordering::SeqCst);
                        state.notifier.send_critical_alert(
                            "CRITICAL_SETTLEMENT_FAILURE",
                            format!(
                                "settleTrade failed {} times consecutively; matching paused. internal_market={:#x}",
                                n, market
                            ),
                            state.telemetry.contract_addresses_snapshot(),
                        );
                    }
                }
            }
        }
    } else {
        for _call in calls {
            state.pending_settlements.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// TPS / Pending / 当前税率 / 经济速率日志输出
async fn run_metrics_logger(state: SharedState) {
    let mut last_orders = 0u64;
    let mut interval = time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        let total = state.order_counter.load(Ordering::Relaxed);
        let tps = total.saturating_sub(last_orders);
        last_orders = total;
        let pending = state.pending_settlements.load(Ordering::Relaxed);
        let tax = state.engine.tax_rate_bps();
        let vel = state.economic_velocity_wei.load(Ordering::Relaxed);
        let inflation_risk = if vel > 0 { "HIGH" } else { "LOW" }; // 占位：真实实现可调用 forecaster.inflation_risk()
        let m = state.telemetry.to_metrics_json();
        info!(
            "[AIDE] [TPS={} orders/s] [Pending={}] [tax_bps={}] [vel_low64={}] [aide_velocity_v={}] [guardrail_paused={}] [INFLATION_RISK:{}]",
            tps,
            pending,
            tax,
            vel,
            m.aide_velocity_v,
            m.aide_guardrail_status,
            inflation_risk
        );
    }
}

/// `paused()` selector — 与 OpenZeppelin Pausable / Guardrail 兼容
async fn fetch_guardrail_paused(
    client: &reqwest::Client,
    rpc_url: &str,
    guardrail: &str,
) -> Result<bool, String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1u64,
        "method": "eth_call",
        "params": [{ "to": guardrail, "data": "0x5c975c06" }, "latest"]
    });
    let res = client
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let result = v["result"]
        .as_str()
        .ok_or_else(|| "no result".to_string())?;
    let raw = result.strip_prefix("0x").unwrap_or(result);
    if raw.len() < 2 {
        return Ok(false);
    }
    let bytes = hex::decode(raw).map_err(|e| e.to_string())?;
    Ok(!bytes.is_empty() && bytes[bytes.len() - 1] != 0)
}
