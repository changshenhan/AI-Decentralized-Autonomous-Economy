use aide_engine::matching::{Fill, MatchEngine};
use aide_engine::orderbook::{OrderRecord, Side};
use alloy_primitives::{address, U256};
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

/// 黑盒压力测试：并发 1000 个 \"SUBMIT_ORDER\" 等价操作，确保撮合与结算队列无死锁。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_limit_orders_do_not_starve() {
    let engine = MatchEngine::new();
    let (fill_tx, mut fill_rx) = mpsc::channel::<Fill>(4096);
    let fill_tx_runner = fill_tx.clone();
    let cid = 42u64;
    let e18 = U256::from(1_000_000_000_000_000_000u128);
    let seller = address!("1111111111111111111111111111111111111111");
    let buyer = address!("2222222222222222222222222222222222222222");

    // 预先挂出卖单深度
    for i in 0..1000u64 {
        let sell = OrderRecord {
            order_id: i,
            company_id: cid,
            maker: seller,
            side: Side::Sell,
            price_ray: e18,
            remaining: U256::from(1u64),
            timestamp: i,
        };
        engine.ingest_chain_order(sell).unwrap();
    }

    // MatchRunner：消费 buy 单，产出 Fill（fill_tx 已 clone 进 runner，避免与外层 drop 冲突）
    let eng_clone = engine.clone();
    let runner = tokio::spawn(async move {
        let start = Instant::now();
        let mut handles = Vec::new();
        for i in 0..1000u64 {
            let eng = eng_clone.clone();
            let tx = fill_tx_runner.clone();
            handles.push(tokio::spawn(async move {
                let buy = OrderRecord {
                    order_id: 10_000 + i,
                    company_id: cid,
                    maker: buyer,
                    side: Side::Buy,
                    price_ray: e18,
                    remaining: U256::from(1u64),
                    timestamp: i,
                };
                let fills = eng.limit_order_placement(buy).unwrap();
                for f in fills {
                    tx.send(f).await.ok();
                }
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        Instant::now().saturating_duration_since(start)
    });

    drop(fill_tx);

    let mut count = 0usize;
    let mut total_notional = U256::ZERO;
    while let Some(f) = fill_rx.recv().await {
        count += 1;
        total_notional = total_notional.saturating_add(f.ait_notional);
    }

    let elapsed = runner.await.unwrap();
    eprintln!(
        "processed {} matches in {:?}, avg {:?}/order",
        count,
        elapsed,
        if count > 0 {
            elapsed / (count as u32)
        } else {
            Duration::ZERO
        }
    );

    assert_eq!(count, 1000);
    assert!(
        elapsed < Duration::from_secs(10),
        "expected <10s on CI/slow runners, got {:?}",
        elapsed
    );
    // 撮合引擎对 1 单位剩余量 × price_ray 的语义下，并发 1000 笔成交聚合名义（与单测订单簿刻度一致）
    assert_eq!(total_notional, U256::from(1000u64));
}
