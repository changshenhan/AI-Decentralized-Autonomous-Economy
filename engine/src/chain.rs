//! Base / InternalMarket 事件：HTTP / WebSocket（`Provider`）轮询 `eth_getLogs`，解码 `OrderPlaced` / `TradeSettled` / `OrderCancelled`。

use alloy_primitives::{Address, B256, U256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::Filter;
use alloy_rpc_types::Log;
use alloy_sol_types::SolEvent;
use alloy_transport_ws::WsConnect;
use thiserror::Error;
use tracing::{debug, info, warn};

use alloy_sol_types::sol;

sol! {
    event OrderPlaced(
        uint256 indexed orderId,
        uint256 indexed companyId,
        address indexed maker,
        uint8 side,
        uint256 priceRay,
        uint256 amount,
        uint256 timestamp,
        uint256 engineHint
    );

    event TradeSettled(
        uint256 indexed companyId,
        uint256 indexed takerOrderId,
        uint256 indexed makerOrderId,
        address buyer,
        address seller,
        uint256 vstkAmount,
        uint256 aitNotional,
        uint256 fee,
        bytes32 settlementId
    );

    event OrderCancelled(uint256 indexed orderId, address indexed maker);
}

/// `OrderPlaced` topic0
pub fn order_placed_topic() -> B256 {
    OrderPlaced::SIGNATURE_HASH
}

/// `TradeSettled` topic0
pub fn trade_settled_topic() -> B256 {
    TradeSettled::SIGNATURE_HASH
}

/// `OrderCancelled` topic0
pub fn order_cancelled_topic() -> B256 {
    OrderCancelled::SIGNATURE_HASH
}

/// 链监听配置（`rpc_url_http` / `ws_url` 二选一；优先 `ws_url` 建立 `Provider`）
#[derive(Clone, Debug)]
pub struct ChainListenerConfig {
    /// InternalMarket 合约地址
    pub market: Address,
    /// HTTP RPC
    pub rpc_url_http: Option<String>,
    /// WebSocket RPC（与 HTTP 二选一即可；若同时提供优先 WS）
    pub ws_url: Option<String>,
    /// 起始块（冷启动回溯）
    pub from_block: u64,
}

impl Default for ChainListenerConfig {
    fn default() -> Self {
        Self {
            market: Address::ZERO,
            rpc_url_http: None,
            ws_url: None,
            from_block: 0,
        }
    }
}

/// 从链上监听器泵出的标准化事件
#[derive(Debug, Clone)]
pub enum MarketChainEvent {
    NewHead {
        number: u64,
    },
    OrderPlaced {
        block_number: u64,
        order_id: u64,
        company_id: u64,
        maker: Address,
        side: u8,
        price_ray: U256,
        amount: U256,
        timestamp: u64,
        engine_hint: U256,
    },
    TradeSettled {
        block_number: u64,
        company_id: u64,
        taker_order_id: u64,
        maker_order_id: u64,
        buyer: Address,
        seller: Address,
        vstk_amount: U256,
        ait_notional: U256,
        fee: U256,
        settlement_id: B256,
    },
    OrderCancelled {
        block_number: u64,
        order_id: u64,
        maker: Address,
    },
}

#[derive(Debug, Error)]
pub enum ChainListenerError {
    #[error("missing ETH_RPC_URL (http) when ws_url not used")]
    MissingRpc,
    #[error("invalid URL: {0}")]
    BadUrl(String),
    #[error("provider error: {0}")]
    Provider(String),
}

fn map_provider_err(e: impl std::fmt::Display) -> ChainListenerError {
    ChainListenerError::Provider(e.to_string())
}

fn process_log(log: &Log) -> Option<MarketChainEvent> {
    let t0 = log.topic0()?;
    let block_number = log.block_number.unwrap_or(0);
    if *t0 == OrderPlaced::SIGNATURE_HASH {
        match OrderPlaced::decode_log(&log.inner, false) {
            Ok(ev) => Some(MarketChainEvent::OrderPlaced {
                block_number,
                order_id: ev.orderId.to::<u64>(),
                company_id: ev.companyId.to::<u64>(),
                maker: ev.maker,
                side: ev.side,
                price_ray: ev.priceRay,
                amount: ev.amount,
                timestamp: ev.timestamp.to::<u64>(),
                engine_hint: ev.engineHint,
            }),
            Err(e) => {
                warn!(%e, "OrderPlaced decode");
                None
            }
        }
    } else if *t0 == TradeSettled::SIGNATURE_HASH {
        match TradeSettled::decode_log(&log.inner, false) {
            Ok(ev) => Some(MarketChainEvent::TradeSettled {
                block_number,
                company_id: ev.companyId.to::<u64>(),
                taker_order_id: ev.takerOrderId.to::<u64>(),
                maker_order_id: ev.makerOrderId.to::<u64>(),
                buyer: ev.buyer,
                seller: ev.seller,
                vstk_amount: ev.vstkAmount,
                ait_notional: ev.aitNotional,
                fee: ev.fee,
                settlement_id: ev.settlementId,
            }),
            Err(e) => {
                warn!(%e, "TradeSettled decode");
                None
            }
        }
    } else if *t0 == OrderCancelled::SIGNATURE_HASH {
        match OrderCancelled::decode_log(&log.inner, false) {
            Ok(ev) => Some(MarketChainEvent::OrderCancelled {
                block_number,
                order_id: ev.orderId.to::<u64>(),
                maker: ev.maker,
            }),
            Err(e) => {
                warn!(%e, "OrderCancelled decode");
                None
            }
        }
    } else {
        None
    }
}

macro_rules! run_poll_loop {
    ($provider:expr, $market:expr, $from:expr, $tx:expr) => {{
        let provider = $provider;
        let market = $market;
        let mut cursor = if $from > 0 {
            $from
        } else {
            provider
                .get_block_number()
                .await
                .map_err(map_provider_err)?
                .saturating_sub(1)
        };

        info!(?market, %cursor, "InternalMarket log listener started (HTTP or WS transport)");

        loop {
            let head = provider.get_block_number().await.map_err(map_provider_err)?;
            if $tx
                .send(MarketChainEvent::NewHead { number: head })
                .await
                .is_err()
            {
                return Err(ChainListenerError::Provider("channel closed".into()));
            }

            while cursor < head {
                cursor += 1;
                let filter = Filter::new()
                    .address(market)
                    .from_block(cursor)
                    .to_block(cursor);
                let logs = provider.get_logs(&filter).await.map_err(map_provider_err)?;
                debug!(block = cursor, n = logs.len(), "eth_getLogs");
                for log in logs.iter() {
                    if let Some(ev) = process_log(log) {
                        if $tx.send(ev).await.is_err() {
                            return Err(ChainListenerError::Provider("channel closed".into()));
                        }
                    }
                }
            }

            info!(block = head, "chain head (InternalMarket listener; Base Sepolia / L2)");
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }};
}

/// HTTP / WS：按块 `eth_getLogs` 增量拉取。
pub async fn run_internal_market_listener(
    cfg: ChainListenerConfig,
    tx: tokio::sync::mpsc::Sender<MarketChainEvent>,
) -> Result<(), ChainListenerError> {
    let market = cfg.market;
    let from = cfg.from_block;
    if let Some(ref ws) = cfg.ws_url {
        let conn = WsConnect::new(ws.clone());
        let provider = ProviderBuilder::new()
            .on_ws(conn)
            .await
            .map_err(|e| ChainListenerError::Provider(e.to_string()))?;
        run_poll_loop!(provider, market, from, tx)
    } else {
        let url_str = cfg
            .rpc_url_http
            .clone()
            .ok_or(ChainListenerError::MissingRpc)?;
        let url: reqwest::Url = url_str
            .parse()
            .map_err(|_| ChainListenerError::BadUrl(url_str))?;
        let provider = ProviderBuilder::new().on_http(url);
        run_poll_loop!(provider, market, from, tx)
    }
}
