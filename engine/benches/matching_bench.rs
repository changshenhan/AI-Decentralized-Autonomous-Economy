//! 本地撮合吞吐基准：目标在消费级 CPU 上达到 **每秒万级** 订单处理（视硬件与编译优化而定）。
use aide_engine::matching::MatchEngine;
use aide_engine::orderbook::{OrderRecord, Side};
use alloy_primitives::{address, U256};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_hot_path(c: &mut Criterion) {
    let e18 = U256::from(1_000_000_000_000_000_000u128);
    let maker = address!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let taker = address!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");

    c.bench_function("match_engine_10k_cross", |b| {
        b.iter(|| {
            let eng = MatchEngine::new();
            let cid = 1u64;
            for i in 0..5000u64 {
                let sell = OrderRecord {
                    order_id: i * 2,
                    company_id: cid,
                    maker,
                    side: Side::Sell,
                    price_ray: e18,
                    remaining: U256::from(1000u64),
                    timestamp: i,
                };
                eng.ingest_chain_order(sell).unwrap();
                let buy = OrderRecord {
                    order_id: i * 2 + 1,
                    company_id: cid,
                    maker: taker,
                    side: Side::Buy,
                    price_ray: e18,
                    remaining: U256::from(1000u64),
                    timestamp: i,
                };
                let fills = eng.limit_order_placement(buy).unwrap();
                black_box(fills.len());
            }
        });
    });
}

criterion_group!(benches, bench_hot_path);
criterion_main!(benches);
