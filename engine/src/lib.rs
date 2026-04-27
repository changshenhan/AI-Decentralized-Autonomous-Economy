//! AIDE Rust Engine — Matcher、链同步、结算器、经济预测占位（TECHNICAL_ARCH_MAP / ENGINE_SYNC_SPEC）
#![deny(unsafe_code)]
#![allow(missing_docs)]

pub mod audit;
pub mod chain;
pub mod chaos;
pub mod forecaster;
pub mod matching;
pub mod notifier;
pub mod orderbook;
pub mod settler;
pub mod sync;
pub mod tee;
pub mod telemetry;

pub use audit::{
    fetch_chain_snapshot_for_shadow, report_exceeds_threshold, run_sync_audit_once,
    sync_treasury_tax_rate_to_engine, AuditError,
};
pub use chain::{
    order_cancelled_topic, order_placed_topic, run_internal_market_listener, trade_settled_topic,
    ChainListenerConfig, ChainListenerError, MarketChainEvent, OrderCancelled, OrderPlaced,
    TradeSettled,
};
pub use matching::{Fill, MatchEngine, MatchError};
pub use orderbook::{OrderBook, OrderRecord, Side};
pub use settler::{
    encode_settle_trade_calldata, load_settler_signer, send_settle_trade_with_retry, SettlerTxError,
};
pub use sync::{ChainBalanceSnapshot, ReconciliationReport, SyncCoordinator};
pub use notifier::NotificationManager;
pub use telemetry::{
    load_contract_address_book_from_env, router as telemetry_router, serve as telemetry_serve,
    ContractAddressBook, DashboardSummaryJson, MatchingPauseReason, MetricsJson, TelemetryHub,
};
