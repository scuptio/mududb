//! Reproducible opt-in baseline/scaling/ablation benchmark, no timing assertions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::{
    tests::{context, key},
    *,
};
use crate::btree::btree_index::BTreeIndex;
use mudu_sys::time::instant_now;
use std::rc::Rc;
use std::{hint::black_box, ops::Bound, sync::Arc};
const KEYS: u64 = 4096;
const OPS: u64 = 10000;

#[tokio::test(flavor = "current_thread")]
#[ignore = "run with --release --ignored --nocapture for index throughput and p99"]
async fn index_local_tasks_and_hint_ablation() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let old = BTreeIndex::new(context());
            for i in 0..KEYS {
                old.insert(key(i), i).unwrap();
            }
            let start = instant_now();
            for i in 0..OPS {
                let n = (i * 73) % KEYS;
                if i % 10 == 0 {
                    old.insert(key(n), n).unwrap();
                } else {
                    black_box(old.get(&key(n)).unwrap());
                }
            }
            println!(
                "baseline single-thread mixed 90% read: {:.0} ops/s",
                OPS as f64 / start.elapsed().as_secs_f64()
            );
            for swizzling in [false, true] {
                for task_count in [1, 2, 4] {
                    run(task_count, swizzling).await;
                }
            }
        })
        .await;
}
async fn run(task_count: u64, swizzling: bool) {
    let tree = Rc::new(
        PagedBTree::with_store(
            context(),
            BufferOptions {
                swizzling,
                ..BufferOptions::default()
            },
            Arc::new(MemoryPageStore::default()),
        )
        .unwrap(),
    );
    for i in 0..KEYS {
        tree.insert(key(i), i).await.unwrap();
    }
    let start = instant_now();
    let mut tasks = Vec::new();
    for worker in 0..task_count {
        let tree = tree.clone();
        tasks.push(tokio::task::spawn_local(async move {
            let mut samples = Vec::new();
            for i in 0..OPS {
                let n = (i * 73 + worker * 997) % KEYS;
                let start = instant_now();
                if i % 10 == 0 {
                    tree.insert(key(n), n).await.unwrap();
                } else {
                    black_box(tree.get(&key(n)).await.unwrap());
                }
                if i % 64 == 63 {
                    tokio::task::yield_now().await;
                }
                if i % 100 == 0 {
                    samples.push(start.elapsed().as_nanos());
                }
            }
            samples
        }));
    }
    let mut samples = Vec::new();
    for task in tasks {
        samples.extend(task.await.unwrap());
    }
    let elapsed = start.elapsed().as_secs_f64();
    samples.sort_unstable();
    let p99 = samples[samples.len() * 99 / 100];
    let start = instant_now();
    for _ in 0..10 {
        black_box(
            tree.range((Bound::Unbounded, Bound::Unbounded))
                .await
                .unwrap(),
        );
    }
    println!("paged mixed 90% read task_count={task_count} hints={swizzling}: {:.0} ops/s sampled_p99_ns={p99} scan_rows/s={:.0} stats={:?}",
        (task_count*OPS) as f64/elapsed,(10*KEYS) as f64/start.elapsed().as_secs_f64(),tree.buffer_stats());
}
