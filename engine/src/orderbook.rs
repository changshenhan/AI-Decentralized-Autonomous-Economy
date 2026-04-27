//! 订单簿：按 `company_id` 分片；单价深度用 `BTreeMap`，同价时间优先用 `VecDeque`（FIFO）。
//! 热路径避免多余克隆：`BookOrder` 以拥有型字段存储，`remaining` 就地扣减。

use alloy_primitives::{Address, U256};
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, VecDeque};

/// 买卖方向（与 Solidity `InternalMarket.Side` 一致：0=Buy, 1=Sell）
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Side {
    Buy = 0,
    Sell = 1,
}

impl From<u8> for Side {
    fn from(v: u8) -> Self {
        if v == 1 {
            Side::Sell
        } else {
            Side::Buy
        }
    }
}

/// 单笔挂单（链上 `OrderPlaced` 对齐）
#[derive(Clone, Debug)]
pub struct OrderRecord {
    pub order_id: u64,
    pub company_id: u64,
    pub maker: Address,
    pub side: Side,
    pub price_ray: U256,
    pub remaining: U256,
    pub timestamp: u64,
}

/// 单公司 CLOB：买盘按价降序（`Reverse(price)`），卖盘按价升序。
#[derive(Default)]
pub struct OrderBook {
    pub(crate) bids: BTreeMap<Reverse<U256>, VecDeque<u64>>,
    pub(crate) asks: BTreeMap<U256, VecDeque<u64>>,
    pub(crate) orders: HashMap<u64, OrderRecord>,
}

impl OrderBook {
    /// 从链上事件插入挂单（零拷贝语义：此处 `record` 由调用方构造，簿内仅存一份）
    pub fn insert_limit_order(&mut self, record: OrderRecord) -> Result<(), BookError> {
        let id = record.order_id;
        if self.orders.contains_key(&id) {
            return Err(BookError::DuplicateOrderId(id));
        }
        let price = record.price_ray;
        match record.side {
            Side::Buy => {
                self.bids.entry(Reverse(price)).or_default().push_back(id);
            }
            Side::Sell => {
                self.asks.entry(price).or_default().push_back(id);
            }
        }
        self.orders.insert(id, record);
        Ok(())
    }

    /// 撤单：与链上 `OrderCancelled` 对齐
    pub fn cancel_order(&mut self, order_id: u64) -> Result<(), BookError> {
        let o = self
            .orders
            .remove(&order_id)
            .ok_or(BookError::UnknownOrder(order_id))?;
        let price = o.price_ray;
        let deque = match o.side {
            Side::Buy => self.bids.get_mut(&Reverse(price)),
            Side::Sell => self.asks.get_mut(&price),
        };
        if let Some(q) = deque {
            if let Some(pos) = q.iter().position(|&x| x == order_id) {
                q.remove(pos);
            }
            if q.is_empty() {
                match o.side {
                    Side::Buy => {
                        self.bids.remove(&Reverse(price));
                    }
                    Side::Sell => {
                        self.asks.remove(&price);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn get_order(&self, id: u64) -> Option<&OrderRecord> {
        self.orders.get(&id)
    }

    pub fn get_order_mut(&mut self, id: u64) -> Option<&mut OrderRecord> {
        self.orders.get_mut(&id)
    }

    /// 最优卖一
    pub fn best_ask(&self) -> Option<U256> {
        self.asks.keys().next().copied()
    }

    /// 最优买一
    pub fn best_bid(&self) -> Option<U256> {
        self.bids.keys().next().map(|r| r.0)
    }

    /// 深度遍历：卖盘从低到高（仅用于调试 / 对账）
    pub fn ask_levels(&self) -> impl Iterator<Item = (U256, &VecDeque<u64>)> {
        self.asks.iter().map(|(p, q)| (*p, q))
    }

    pub fn bid_levels(&self) -> impl Iterator<Item = (U256, &VecDeque<u64>)> {
        self.bids.iter().map(|(r, q)| (r.0, q))
    }
}

/// 订单簿错误
#[derive(Debug, thiserror::Error)]
pub enum BookError {
    #[error("duplicate order id: {0}")]
    DuplicateOrderId(u64),
    #[error("unknown order: {0}")]
    UnknownOrder(u64),
}
