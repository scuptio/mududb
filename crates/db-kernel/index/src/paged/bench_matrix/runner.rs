//! Setup, synchronized tasks, and machine-readable measurements for the matrix.
use super::*;
use mudu_sys::contract::file_options::FileOptions;
use std::rc::Rc;
use std::{hint::black_box, path::PathBuf};
use tokio::sync::Barrier;

struct Prepared {
    tree: Rc<Tree>,
    file: Option<PathBuf>,
    load_ms: f64,
    flush_ms: f64,
    pages: u64,
}
async fn prepare(case: &Case, keys: &[KeyTuple], paged: bool) -> Prepared {
    let mut file = None;
    let mut store: Arc<dyn PageStore> = Arc::new(MemoryPageStore::default());
    if paged && case.file {
        let directory =
            mudu_sys::env_var::var("MUDU_BENCH_DIR").expect("file cases require MUDU_BENCH_DIR");
        let path = PathBuf::from(directory).join("pages.bin");
        let fs = mudu_sys::default_sys_io_context().fs();
        let options = FileOptions {
            create_new: true,
            ..FileOptions::read_write_create()
        };
        let backing = fs.open(&path, options).await.unwrap();
        store = Arc::new(FilePageStore::new(backing, case.page_size).unwrap());
        file = Some(path);
    }
    let options = BufferOptions {
        page_size: case.page_size,
        frames: case.frames,
        partitions: 1,
        swizzling: case.hints,
        prefetch: case.prefetch,
    };
    let mut tree = if paged {
        let tree = if case.generic {
            PagedBTree::with_store(context(), options, store.clone()).unwrap()
        } else {
            PagedBTree::with_row_ids(context(), options, store.clone()).unwrap()
        };
        let tree = if mudu_sys::env_var::var("MUDU_BENCH_DIAGNOSTICS").is_some() {
            tree.with_diagnostics()
        } else {
            tree
        };
        Tree::Paged(Box::new(if case.lexical {
            tree.with_lexicographic_keys()
        } else {
            tree
        }))
    } else {
        Tree::Baseline(Box::new(BTreeIndex::new(context())))
    };
    let start = instant_now();
    for (id, key) in keys[..case.keys].iter().enumerate() {
        assert_eq!(tree.insert(key.clone(), id as u128).await, None);
    }
    let load_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = instant_now();
    let mut pages = 0;
    if let Tree::Paged(t) = &tree {
        if case.generic {
            let access = t.buffer.maintenance().await;
            t.buffer.flush(&access).await.unwrap();
        } else {
            let state = t.checkpoint().await.unwrap();
            pages = state.next_page - 1;
            if case.cold {
                tree = Tree::Paged(Box::new(
                    PagedBTree::restore_row_ids(context(), options, store, &state)
                        .await
                        .unwrap(),
                ));
            }
        }
    }
    let flush_ms = start.elapsed().as_secs_f64() * 1000.0;
    // Restoring an empty cache constructs a fresh tree; diagnostic capability is
    // intentionally process-local and must be enabled again after restore.
    if case.cold && mudu_sys::env_var::var("MUDU_BENCH_DIAGNOSTICS").is_some() {
        if let Tree::Paged(t) = tree {
            tree = Tree::Paged(Box::new(t.with_diagnostics()));
        }
    }
    // Warmup only reads, so inserts/deletes start from identical logical states.
    // Cold-start means empty application cache, not an empty OS page cache.
    if !case.cold {
        for id in 0..if super::cache_cases::enabled() {
            case.keys
        } else {
            512.min(case.keys)
        } {
            let id = if super::cache_cases::enabled() {
                id
            } else {
                id * 73 % case.keys
            };
            assert_eq!(tree.get(&keys[id]).await, Some(id as u128));
        }
    }
    Prepared {
        tree: Rc::new(tree),
        file,
        load_ms,
        flush_ms,
        pages,
    }
}

#[derive(Clone, Copy)]
enum Op {
    Read(usize),
    Miss(usize),
    Update(usize),
    Changed(usize, u128, u128),
    Insert(usize),
    Delete(usize),
    Scan(usize, usize),
}
fn trace(case: &Case, ops: usize, worker: usize, trial: usize) -> Vec<Op> {
    let mut seed = 0x123456789abcdef ^ ((worker as u64 + 1) * 997) ^ ((trial as u64 + 1) * 73);
    let mut values: Vec<u128> = (0..case.keys).map(|id| id as u128).collect();
    (0..ops)
        .map(|i| {
            let id = random(&mut seed) as usize % if case.hot { 16 } else { case.keys };
            let write = random(&mut seed) % 100;
            match case.workload {
                "changed-update" => {
                    // Disjoint worker ranges make old-value assertions deterministic.
                    let width = case.keys / case.tasks;
                    let id = worker * width + id % width;
                    let next = ((i as u128 + 1) << 64) | id as u128;
                    let old = std::mem::replace(&mut values[id], next);
                    Op::Changed(id, old, next)
                }
                "read" => Op::Read(id),
                "miss" => Op::Miss(case.keys + id),
                "mixed90" if write < 10 => Op::Update(id),
                "mixed50" if write < 50 => Op::Update(id),
                "mixed90" | "mixed50" => Op::Read(id),
                "update" => Op::Update(id),
                "insert" => Op::Insert(case.keys + worker * ops + i),
                "delete" => Op::Delete(worker * ops + i),
                "scan32" => Op::Scan(id % (case.keys - 32), 32),
                "scan256" => Op::Scan(id % (case.keys - 256), 256),
                _ => panic!("unknown workload"),
            }
        })
        .collect()
}
async fn perform(tree: &Tree, keys: &[KeyTuple], op: Op) -> usize {
    match op {
        Op::Read(id) => {
            assert_eq!(black_box(tree.get(&keys[id]).await), Some(id as u128));
            1
        }
        Op::Miss(id) => {
            assert_eq!(black_box(tree.get(&keys[id]).await), None);
            0
        }
        Op::Update(id) => {
            assert_eq!(
                tree.insert(keys[id].clone(), id as u128).await,
                Some(id as u128)
            );
            1
        }
        Op::Changed(id, old, next) => {
            assert_eq!(tree.insert(keys[id].clone(), next).await, Some(old));
            1
        }
        Op::Insert(id) => {
            assert_eq!(tree.insert(keys[id].clone(), id as u128).await, None);
            1
        }
        Op::Delete(id) => {
            assert_eq!(tree.remove(&keys[id]).await, Some(id as u128));
            1
        }
        Op::Scan(id, width) => {
            let rows = black_box(tree.range(&keys[id], &keys[id + width]).await);
            assert_eq!(rows.len(), width);
            assert_eq!(rows.first().unwrap().1, id as u128);
            assert_eq!(rows.last().unwrap().1, (id + width - 1) as u128);
            rows.len()
        }
    }
}

struct Measurement {
    seconds: f64,
    samples: Vec<u64>,
    rows: usize,
}
async fn measure(tree: &Rc<Tree>, keys: Arc<Vec<KeyTuple>>, traces: Vec<Vec<Op>>) -> Measurement {
    let barrier = Arc::new(Barrier::new(traces.len() + 1));
    let mut tasks = Vec::new();
    for trace in traces {
        let (tree, keys, barrier) = (tree.clone(), keys.clone(), barrier.clone());
        tasks.push(tokio::task::spawn_local(async move {
            let mut samples = Vec::with_capacity(trace.len());
            let mut rows = 0;
            barrier.wait().await;
            for (i, op) in trace.into_iter().enumerate() {
                let start = instant_now();
                rows += perform(&tree, &keys, op).await;
                // Every operation is timed, avoiding periodic sampling bias toward writes.
                samples.push(start.elapsed().as_nanos() as u64);
                // Equal cooperative scheduling for synchronous baseline and resident paged paths.
                if i % 64 == 63 {
                    tokio::task::yield_now().await;
                }
            }
            (samples, rows)
        }));
    }
    let start = instant_now();
    barrier.wait().await;
    let mut result = Measurement {
        seconds: 0.0,
        samples: Vec::new(),
        rows: 0,
    };
    for task in tasks {
        let (samples, rows) = task.await.unwrap();
        result.samples.extend(samples);
        result.rows += rows;
    }
    result.seconds = start.elapsed().as_secs_f64();
    result.samples.sort_unstable();
    result
}

pub(super) async fn run(case: &Case, requested_ops: usize, trial: usize, paged: bool) {
    let ops = if case.workload == "delete" {
        requested_ops.min(case.keys / case.tasks)
    } else {
        requested_ops
    };
    // Only insert traces need an expanded key domain. Keep setup proportional
    // to the data set for long CPU-sampling runs of read/update workloads.
    let count = if case.workload == "insert" {
        case.keys + case.tasks * ops + 1
    } else {
        case.keys * 2 + 1
    };
    let keys = Arc::new(
        (0..count)
            .map(|id| key(id, case.key_bytes, case.prefix))
            .collect::<Vec<_>>(),
    );
    let traces: Vec<Vec<Op>> = (0..case.tasks)
        .map(|worker| trace(case, ops, worker, trial))
        .collect();
    let mut final_values: Vec<u128> = (0..count).map(|id| id as u128).collect();
    for op in traces.iter().flatten() {
        if let Op::Changed(id, _, next) = op {
            final_values[*id] = *next;
        }
    }
    let prepared = prepare(case, &keys, paged).await;
    let before = prepared.tree.stats();
    let diagnostic = |tree: &Tree| match tree {
        Tree::Paged(t) => t.performance_stats(),
        _ => PerformanceStats::default(),
    };
    let d0 = diagnostic(&prepared.tree);
    let window = super::instrumentation::Window::start();
    let result = measure(&prepared.tree, keys.clone(), traces).await;
    window.finish(case, paged, trial);
    let d1 = diagnostic(&prepared.tree);
    report_diagnostics(case, trial, paged, d0, d1);
    let after = prepared.tree.stats();
    super::cache_cases::report(
        case,
        trial,
        paged,
        prepared.pages,
        result.samples.len(),
        before,
        after,
    );
    let expected = match case.workload {
        "insert" => case.keys + ops * case.tasks,
        "delete" => case.keys - ops * case.tasks,
        _ => case.keys,
    };
    assert_eq!(prepared.tree.len().await, expected);
    // Validate the complete final mapping outside timing, including inserted/deleted keys.
    let final_rows = prepared.tree.range(&keys[0], keys.last().unwrap()).await;
    assert_eq!(final_rows.len(), expected);
    let first = if case.workload == "delete" {
        ops * case.tasks
    } else {
        0
    };
    for (offset, (stored_key, value)) in final_rows.iter().enumerate() {
        let id = first + offset;
        assert_eq!(stored_key.as_slice(), keys[id].as_slice());
        assert_eq!(*value, final_values[id]);
    }
    report(case, trial, paged, &prepared, &result, before, after);
    // Drop all page/file handles before deleting only the file created by this run.
    drop(prepared.tree);
    if let Some(path) = prepared.file {
        mudu_sys::default_sys_io_context()
            .fs()
            .remove_file_if_exists(&path)
            .await
            .unwrap();
    }
}
fn report(
    case: &Case,
    trial: usize,
    paged: bool,
    prepared: &Prepared,
    m: &Measurement,
    before: BufferStats,
    after: BufferStats,
) {
    let percentile =
        |p: usize| m.samples[(m.samples.len() * p).div_ceil(100).saturating_sub(1)] as f64 / 1000.0;
    let backend = if paged { "paged" } else { "baseline" };
    // Frame bytes are a configured payload budget, not a measurement of RSS or total memory.
    println!("BENCH,{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{:.6},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{},{},{},{}",
        case.name, backend, trial, case.workload, case.keys, case.tasks,
        case.key_bytes, case.prefix, case.hot, case.page_size, case.frames,
        case.hints, case.prefetch, case.file && paged, case.cold, case.generic, case.lexical,
        m.seconds, m.samples.len() as f64 / m.seconds, percentile(50), percentile(95), percentile(99),
        prepared.load_ms, if paged { prepared.flush_ms } else { 0.0 },
        after.reads - before.reads, after.writes - before.writes,
        after.evictions - before.evictions, after.hint_hits - before.hint_hits, m.rows);
}

fn report_diagnostics(
    case: &Case,
    trial: usize,
    paged: bool,
    d0: PerformanceStats,
    d1: PerformanceStats,
) {
    if mudu_sys::env_var::var("MUDU_BENCH_DIAGNOSTICS").is_some() {
        println!(
            "DIAG,{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            case.name,
            if paged { "paged" } else { "baseline" },
            trial,
            d1.retries - d0.retries,
            d1.cow_bytes - d0.cow_bytes,
            d1.moved_bytes - d0.moved_bytes,
            d1.overflow_reads - d0.overflow_reads,
            d1.latch_waits - d0.latch_waits,
            d1.latch_wait_ns - d0.latch_wait_ns,
            d1.latch_probe_hits - d0.latch_probe_hits,
            d1.checksum_calls - d0.checksum_calls,
            d1.checksum_bytes - d0.checksum_bytes,
            d1.checksum_ns - d0.checksum_ns,
            d1.writeback_copy_bytes - d0.writeback_copy_bytes
        );
    }
}
