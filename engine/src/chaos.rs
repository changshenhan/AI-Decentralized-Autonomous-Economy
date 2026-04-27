//! 混沌注入：撮合/结算压力测试用的随机抖动与丢单（第九阶段）
//!
//! 用于在测试或基准中引入不确定性，验证引擎在无序输入下的健壮性。

use rand::{rngs::StdRng, Rng, SeedableRng};

/// 可复现的 RNG（固定 seed 便于回归）
pub fn chaos_rng(seed: u64) -> StdRng {
    StdRng::seed_from_u64(seed)
}

/// 在 `[base, base + spread]` 上均匀抖动（毫秒级延迟模拟）
pub fn jitter_delay_ms<R: Rng>(rng: &mut R, base: u64, spread: u64) -> u64 {
    base + rng.gen_range(0..=spread)
}

/// 以概率 `p`（0.0–1.0）模拟丢单 / 无效报单
pub fn maybe_drop<R: Rng>(rng: &mut R, p: f64) -> bool {
    let p = p.clamp(0.0, 1.0);
    rng.gen::<f64>() < p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_in_range() {
        let mut r = chaos_rng(42);
        for _ in 0..100 {
            let j = jitter_delay_ms(&mut r, 10, 5);
            assert!((10..=15).contains(&j));
        }
    }
}
