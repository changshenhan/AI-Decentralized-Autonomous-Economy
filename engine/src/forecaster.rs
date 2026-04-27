//! AI 经济员占位：估算 $AIT$ 流通速率（velocity），阈值触发时由运维调用链上 `AI_Economist_Controller`。
//!
//! **注意**：真实调控须由多签 / `AI_Economist_Controller` owner 执行；本模块仅做指标与告警。

use alloy_primitives::U256;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// 滑动窗口内累计成交量（影子，可由 `TradeSettled` 事件驱动 `record`）
#[derive(Debug)]
pub struct VelocityWindow {
    window: Duration,
    samples: VecDeque<(Instant, U256)>,
    sum: U256,
}

impl VelocityWindow {
    /// 创建窗口（例如 24h 由上层换算为秒）
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            samples: VecDeque::new(),
            sum: U256::ZERO,
        }
    }

    /// 记录一笔成交名义 AIT（`aitNotional`）
    pub fn record(&mut self, amount: U256) {
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

    /// 当前窗口内总量（零拷贝：返回引用聚合值）
    pub fn window_volume(&mut self) -> U256 {
        self.evict_old(Instant::now());
        self.sum
    }
}

/// 调控阈值：当 `velocity > threshold` 时建议人工复核税率/系数（不自动发链上交易）
#[derive(Clone, Debug)]
pub struct ForecasterConfig {
    /// 超过该速率（wei / 窗口）则触发 `on_high_velocity`
    pub velocity_threshold: U256,
    pub window: Duration,
    /// 估算 MV=PY 中的价格水平 P（简化为常数，占位）
    pub price_level_p: U256,
    /// 名义产出 Y（简化为目标值，占位）
    pub output_y: U256,
    /// 当 V 超过此值视为 HIGH inflation risk
    pub v_high_threshold: U256,
}

/// 经济预测服务（占位）
pub struct EconomicForecaster {
    cfg: ForecasterConfig,
    vel: VelocityWindow,
    /// 估算的货币供应量 M（可由国库余额+流通量估算，此处占位）
    pub money_supply_m: U256,
}

impl EconomicForecaster {
    /// 新建
    pub fn new(cfg: ForecasterConfig) -> Self {
        let w = cfg.window;
        Self {
            cfg,
            vel: VelocityWindow::new(w),
            money_supply_m: U256::from(1u64), // 占位，后续可由链上视图实时更新
        }
    }

    /// 从成交回填速率估计
    pub fn on_trade_volume(&mut self, ait_notional: U256) {
        self.vel.record(ait_notional);
    }

    /// 是否达到「需关注」阈值（返回 true 时应由运维调用 `AI_Economist_Controller`）
    pub fn should_alert(&mut self) -> bool {
        self.vel.window_volume() > self.cfg.velocity_threshold
    }

    /// 简易 MV=PY 模型下的速度估计：V = P*Y / M
    pub fn estimate_velocity_mvpy(&mut self) -> U256 {
        let m = self.money_supply_m;
        if m.is_zero() {
            return U256::ZERO;
        }
        (self.cfg.price_level_p * self.cfg.output_y) / m
    }

    /// 基于 V 与配置判断通胀风险标签
    pub fn inflation_risk(&mut self) -> InflationRisk {
        let v = self.estimate_velocity_mvpy();
        if v > self.cfg.v_high_threshold {
            InflationRisk::High
        } else {
            InflationRisk::Low
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum InflationRisk {
    High,
    Low,
}
