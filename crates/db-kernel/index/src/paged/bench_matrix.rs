//! Opt-in, matched-trace performance experiments; no machine-dependent thresholds.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[cfg(feature = "bench-alloc")]
mod allocations;
mod cache_cases;
mod instrumentation;
mod runner;
mod task_cases;
use super::{tests::context, *};
use crate::{btree::btree_index::BTreeIndex, index_key::key_tuple::KeyTuple};
use mudu_sys::time::instant_now;
use std::{ops::Bound, sync::Arc};

/// Each case changes one axis from the baseline unless its name says otherwise.
#[derive(Clone)]
struct Case {
    name: String,
    workload: &'static str,
    keys: usize,
    tasks: usize,
    key_bytes: usize,
    prefix: bool,
    hot: bool,
    page_size: usize,
    frames: usize,
    hints: bool,
    prefetch: bool,
    file: bool,
    cold: bool,
    generic: bool,
    lexical: bool,
}
impl Default for Case {
    fn default() -> Self {
        Self {
            name: "read".into(),
            workload: "read",
            keys: 4096,
            tasks: 1,
            key_bytes: 8,
            prefix: false,
            hot: false,
            page_size: 16384,
            frames: 256,
            hints: true,
            prefetch: true,
            file: false,
            cold: false,
            generic: false,
            lexical: false,
        }
    }
}
fn cases() -> Vec<Case> {
    match mudu_sys::env_var::var("MUDU_BENCH_SUITE").as_deref() {
        Some("cache") => cache_cases::cases(),
        Some("tasks") => task_cases::cases(),
        _ => matrix_cases(),
    }
}
fn matrix_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for workload in [
        "read", "miss", "mixed90", "mixed50", "update", "insert", "delete", "scan32", "scan256",
    ] {
        cases.push(Case {
            name: workload.into(),
            workload,
            ..Case::default()
        });
    }
    for tasks in [2, 4] {
        for workload in ["read", "mixed90", "update"] {
            cases.push(Case {
                name: format!("{workload}-t{tasks}"),
                tasks,
                workload,
                ..Case::default()
            });
        }
    }
    for keys in [1024, 32768] {
        cases.push(Case {
            name: format!("keys-{keys}"),
            keys,
            ..Case::default()
        });
    }
    for key_bytes in [128, 2048] {
        cases.push(Case {
            name: format!("key-bytes-{key_bytes}"),
            key_bytes,
            ..Case::default()
        });
    }
    cases.push(Case {
        name: "prefix-128".into(),
        key_bytes: 128,
        prefix: true,
        ..Case::default()
    });
    for page_size in [4096, 32768] {
        cases.push(Case {
            name: format!("page-{page_size}"),
            page_size,
            frames: 4194304 / page_size,
            ..Case::default()
        });
    }
    for frames in [16, 64, 256] {
        cases.push(Case {
            name: format!("frames-{frames}"),
            frames,
            page_size: 4096,
            keys: 8192,
            ..Case::default()
        });
    }
    cases.push(Case {
        name: "hot16-t4".into(),
        hot: true,
        tasks: 4,
        workload: "mixed90",
        ..Case::default()
    });
    cases.push(Case {
        name: "hints-off".into(),
        hints: false,
        ..Case::default()
    });
    cases.push(Case {
        name: "prefetch-off".into(),
        prefetch: false,
        workload: "scan256",
        ..Case::default()
    });
    cases.push(Case {
        name: "generic-values".into(),
        generic: true,
        ..Case::default()
    });
    for cold in [false, true] {
        cases.push(Case {
            name: format!("file-{}", if cold { "cold-start" } else { "pressure" }),
            file: true,
            cold,
            frames: 16,
            page_size: 4096,
            keys: 8192,
            ..Case::default()
        });
    }
    optimization_cases(&mut cases);
    cases
}

/// Additional cases isolate comparator capability and changing-value replacements.
fn optimization_cases(cases: &mut Vec<Case>) {
    for key_bytes in [8, 2048] {
        cases.push(Case {
            name: format!("lexical-{key_bytes}"),
            lexical: true,
            key_bytes,
            ..Case::default()
        });
    }
    for tasks in [1, 4] {
        cases.push(Case {
            name: format!("changed-update-t{tasks}"),
            workload: "changed-update",
            tasks,
            ..Case::default()
        });
    }
}
fn number(name: &str, default: usize) -> usize {
    mudu_sys::env_var::var(name)
        .map(|v| v.parse().expect("invalid benchmark integer"))
        .unwrap_or(default)
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "release performance matrix; run script/bench/paged_index.py"]
async fn performance_matrix() {
    println!("MODEL,partition-local-v1");
    tokio::task::LocalSet::new()
        .run_until(async {
            let repeats = number("MUDU_BENCH_REPEATS", 3);
            let ops = number("MUDU_BENCH_OPS", 3000);
            assert!(repeats > 0 && ops > 0);
            let filter = mudu_sys::env_var::var("MUDU_BENCH_CASE");
            let mut matched = 0;
            for case in cases()
                .into_iter()
                .filter(|c| filter.as_ref().is_none_or(|f| c.name == *f))
            {
                matched += 1;
                for trial in 0..repeats {
                    // Alternate order to reduce systematic warm-host bias between backends.
                    for paged in if trial % 2 == 0 {
                        [false, true]
                    } else {
                        [true, false]
                    } {
                        if let Some(backend) = mudu_sys::env_var::var("MUDU_BENCH_BACKEND") {
                            assert!(backend == "paged" || backend == "baseline");
                            if paged != (backend == "paged") {
                                continue;
                            }
                        }
                        runner::run(&case, ops, trial, paged).await;
                    }
                }
            }
            assert!(matched > 0, "unknown benchmark case");
        })
        .await;
}

/// Deterministic nonzero PRNG; traces are generated outside the measured region.
fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}
fn key(id: usize, size: usize, prefix: bool) -> KeyTuple {
    let mut bytes = vec![0; size];
    if prefix {
        bytes[size - 8..].copy_from_slice(&(id as u64).to_be_bytes());
    } else {
        bytes[..8].copy_from_slice(&(id as u64).to_be_bytes());
        let mut seed = id as u64 + 1;
        for byte in &mut bytes[8..] {
            *byte = random(&mut seed) as u8;
        }
    }
    KeyTuple::from(bytes)
}

/// Both backends are owned by one partition thread; baseline operations never suspend.
enum Tree {
    Baseline(Box<BTreeIndex<u128>>),
    Paged(Box<PagedBTree<u128>>),
}
impl Tree {
    async fn insert(&self, key: KeyTuple, value: u128) -> Option<u128> {
        match self {
            Self::Baseline(t) => t.insert(key, value).unwrap(),
            Self::Paged(t) => t.insert(key, value).await.unwrap(),
        }
    }
    async fn get(&self, key: &KeyTuple) -> Option<u128> {
        match self {
            Self::Baseline(t) => t.get(key).unwrap(),
            Self::Paged(t) => t.get(key).await.unwrap(),
        }
    }
    async fn remove(&self, key: &KeyTuple) -> Option<u128> {
        match self {
            Self::Baseline(t) => t.remove(key).unwrap(),
            Self::Paged(t) => t.remove(key).await.unwrap(),
        }
    }
    async fn range(&self, low: &KeyTuple, high: &KeyTuple) -> Vec<(KeyTuple, u128)> {
        let bounds = (Bound::Included(low), Bound::Excluded(high));
        match self {
            Self::Baseline(t) => t.range(bounds).unwrap(),
            Self::Paged(t) => t.range(bounds).await.unwrap(),
        }
    }
    async fn len(&self) -> usize {
        match self {
            Self::Baseline(t) => t.len().unwrap(),
            Self::Paged(t) => t.len().await.unwrap(),
        }
    }
    fn stats(&self) -> BufferStats {
        match self {
            Self::Baseline(_) => BufferStats::default(),
            Self::Paged(t) => t.buffer_stats(),
        }
    }
}
