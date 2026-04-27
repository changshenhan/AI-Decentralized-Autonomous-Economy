//! 撮合：价格优先、时间优先（同价 FIFO）。`MatchEngine` 按 `company_id` 分片，使用 `DashMap` + 每簿 `parking_lot::Mutex`。

use crate::orderbook::{BookError, OrderBook, OrderRecord, Side};
use alloy_primitives::{Address, U256};
use dashmap::DashMap;
use parking_lot::Mutex;
use std::cmp::Reverse;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// 单笔成交（供 `settleTrade` 打包）
#[derive(Clone, Debug)]
pub struct Fill {
    /// 公司 ID
    pub company_id: u64,
    /// 挂单（对手方）订单 ID
    pub maker_order_id: u64,
    /// 吃单方订单 ID
    pub taker_order_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub vstk_amount: U256,
    pub ait_notional: U256,
}

#[derive(Debug, thiserror::Error)]
pub enum MatchError {
    #[error("book error: {0}")]
    Book(#[from] BookError),
    #[error("matching paused by Sync-Audit / operator")]
    MatchingPaused,
}

/// 全局撮合状态：`company_id -> OrderBook`
#[derive(Clone)]
pub struct MatchEngine {
    books: Arc<DashMap<u64, Mutex<OrderBook>>>,
    matching_paused: Arc<AtomicBool>,
    /// 与链上 `Treasury.getCurrentTaxRate()` 对齐的基点税率（由 Sync-Audit 周期拉取）
    tax_rate_bps: Arc<AtomicU64>,
}

impl Default for MatchEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MatchEngine {
    /// 创建引擎
    pub fn new() -> Self {
        Self {
            books: Arc::new(DashMap::new()),
            matching_paused: Arc::new(AtomicBool::new(false)),
            tax_rate_bps: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 链上 `Treasury.getCurrentTaxRate` 同步值（基点）；用于链下税费估算与遥测，须与合约一致
    pub fn set_tax_rate_bps(&self, bps: u64) {
        self.tax_rate_bps.store(bps, Ordering::Relaxed);
    }

    pub fn tax_rate_bps(&self) -> u64 {
        self.tax_rate_bps.load(Ordering::Relaxed)
    }

    /// Sync-Audit 发现链上/影子漂移时调用，立即停止新撮合
    pub fn pause_matching(&self) {
        self.matching_paused.store(true, Ordering::SeqCst);
    }

    /// 人工恢复（需完成对账与状态再同步后再调用）
    pub fn resume_matching(&self) {
        self.matching_paused.store(false, Ordering::SeqCst);
    }

    pub fn is_matching_paused(&self) -> bool {
        self.matching_paused.load(Ordering::SeqCst)
    }

    /// 注册或更新来自链上的挂单（`OrderPlaced`）
    pub fn ingest_chain_order(&self, record: OrderRecord) -> Result<(), MatchError> {
        let cid = record.company_id;
        let cell = self
            .books
            .entry(cid)
            .or_insert_with(|| Mutex::new(OrderBook::default()));
        cell.lock().insert_limit_order(record)?;
        Ok(())
    }

    /// 链上撤单
    pub fn ingest_cancel(&self, company_id: u64, order_id: u64) -> Result<(), MatchError> {
        let cell = self
            .books
            .entry(company_id)
            .or_insert_with(|| Mutex::new(OrderBook::default()));
        cell.lock().cancel_order(order_id)?;
        Ok(())
    }

    /// 限价单撮合：可与对手盘交叉则成交，剩余留在簿上。
    pub fn limit_order_placement(
        &self,
        mut incoming: OrderRecord,
    ) -> Result<Vec<Fill>, MatchError> {
        if self.is_matching_paused() {
            return Err(MatchError::MatchingPaused);
        }
        let cid = incoming.company_id;
        let mut fills = Vec::new();
        let cell = self
            .books
            .entry(cid)
            .or_insert_with(|| Mutex::new(OrderBook::default()));
        let mut book = cell.lock();

        match incoming.side {
            Side::Buy => match_buy(&mut book, &mut incoming, &mut fills)?,
            Side::Sell => match_sell(&mut book, &mut incoming, &mut fills)?,
        }

        if incoming.remaining > U256::ZERO {
            book.insert_limit_order(incoming)?;
        }
        Ok(fills)
    }
}

fn match_buy(
    book: &mut OrderBook,
    incoming: &mut OrderRecord,
    fills: &mut Vec<Fill>,
) -> Result<(), MatchError> {
    let bid_price = incoming.price_ray;
    let OrderBook {
        ref mut asks,
        ref mut orders,
        ..
    } = book;

    loop {
        let best = asks.keys().next().copied();
        let best = match best {
            Some(p) => p,
            None => break,
        };
        if best > bid_price {
            break;
        }

        loop {
            let oid = {
                let q = asks.get_mut(&best).expect("level");
                q.front().copied()
            };
            let Some(oid) = oid else {
                asks.remove(&best);
                break;
            };

            let rem_in = incoming.remaining;
            if rem_in.is_zero() {
                break;
            }

            let trade_qty = {
                let mo = orders.get_mut(&oid).expect("order");
                min_u256(rem_in, mo.remaining)
            };
            let notional = mul_div_ray(trade_qty, best);
            let buyer = incoming.maker;
            let seller = orders.get(&oid).expect("order").maker;

            fills.push(Fill {
                company_id: incoming.company_id,
                maker_order_id: oid,
                taker_order_id: incoming.order_id,
                buyer,
                seller,
                vstk_amount: trade_qty,
                ait_notional: notional,
            });

            {
                let mo = orders.get_mut(&oid).expect("order");
                mo.remaining = mo.remaining.saturating_sub(trade_qty);
                incoming.remaining = incoming.remaining.saturating_sub(trade_qty);
                if mo.remaining.is_zero() {
                    let q = asks.get_mut(&best).expect("level");
                    if q.front() == Some(&oid) {
                        q.pop_front();
                    } else {
                        q.retain(|&x| x != oid);
                    }
                    orders.remove(&oid);
                }
            }

            if incoming.remaining.is_zero() {
                break;
            }
        }

        if asks.get(&best).map(|q| q.is_empty()).unwrap_or(true) {
            asks.remove(&best);
        }

        if incoming.remaining.is_zero() {
            break;
        }
    }
    Ok(())
}

fn match_sell(
    book: &mut OrderBook,
    incoming: &mut OrderRecord,
    fills: &mut Vec<Fill>,
) -> Result<(), MatchError> {
    let ask_price = incoming.price_ray;
    let OrderBook {
        ref mut bids,
        ref mut orders,
        ..
    } = book;

    loop {
        let best = bids.keys().next().map(|r| r.0);
        let best = match best {
            Some(p) => p,
            None => break,
        };
        if best < ask_price {
            break;
        }
        let key = Reverse(best);

        loop {
            let oid = {
                let q = bids.get_mut(&key).expect("level");
                q.front().copied()
            };
            let Some(oid) = oid else {
                bids.remove(&key);
                break;
            };

            let rem_in = incoming.remaining;
            if rem_in.is_zero() {
                break;
            }

            let trade_qty = {
                let mo = orders.get_mut(&oid).expect("order");
                min_u256(rem_in, mo.remaining)
            };
            let notional = mul_div_ray(trade_qty, best);
            let buyer = orders.get(&oid).expect("order").maker;
            let seller = incoming.maker;

            fills.push(Fill {
                company_id: incoming.company_id,
                maker_order_id: oid,
                taker_order_id: incoming.order_id,
                buyer,
                seller,
                vstk_amount: trade_qty,
                ait_notional: notional,
            });

            {
                let mo = orders.get_mut(&oid).expect("order");
                mo.remaining = mo.remaining.saturating_sub(trade_qty);
                incoming.remaining = incoming.remaining.saturating_sub(trade_qty);
                if mo.remaining.is_zero() {
                    let q = bids.get_mut(&key).expect("level");
                    if q.front() == Some(&oid) {
                        q.pop_front();
                    } else {
                        q.retain(|&x| x != oid);
                    }
                    orders.remove(&oid);
                }
            }

            if incoming.remaining.is_zero() {
                break;
            }
        }

        if bids.get(&key).map(|q| q.is_empty()).unwrap_or(true) {
            bids.remove(&key);
        }

        if incoming.remaining.is_zero() {
            break;
        }
    }
    Ok(())
}

#[inline]
fn min_u256(a: U256, b: U256) -> U256 {
    if a < b {
        a
    } else {
        b
    }
}

/// vSTK * price_ray / 1e18
#[inline]
fn mul_div_ray(qty: U256, price_ray: U256) -> U256 {
    const E18: u128 = 1_000_000_000_000_000_000;
    (qty * price_ray) / U256::from(E18)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;

    #[test]
    fn price_time_priority_cross() {
        let eng = MatchEngine::new();
        let cid = 1u64;
        let e18 = U256::from(1_000_000_000_000_000_000u128);
        let sell1 = OrderRecord {
            order_id: 1,
            company_id: cid,
            maker: address!("1111111111111111111111111111111111111111"),
            side: Side::Sell,
            price_ray: U256::from(2u64) * e18,
            remaining: U256::from(10u64),
            timestamp: 1,
        };
        eng.ingest_chain_order(sell1).unwrap();
        let buy = OrderRecord {
            order_id: 2,
            company_id: cid,
            maker: address!("2222222222222222222222222222222222222222"),
            side: Side::Buy,
            price_ray: U256::from(3u64) * e18,
            remaining: U256::from(10u64),
            timestamp: 2,
        };
        let fills = eng.limit_order_placement(buy).unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].vstk_amount, U256::from(10u64));
    }
}
