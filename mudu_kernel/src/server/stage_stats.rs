//! Lightweight per-stage latency accounting for performance analysis.
//!
//! When `MUDU_STAGE_STATS=1` is set in the environment, selected code spans
//! (SQL parse/bind/plan/run, lock waits, commit path, partition RPC, frame
//! handling) are timed with an RAII [`StageGuard`] and accumulated into
//! thread-local counters. Each worker event loop periodically calls
//! [`dump_if_due`], which logs the aggregated numbers and resets them.
//!
//! Besides wall-clock time, each span also accumulates the calling thread's
//! CPU time (Linux `CLOCK_THREAD_CPUTIME_ID` via
//! [`thread_cpu_time_now`]), so a stage dominated by lock/IO/scheduler waits
//! (high wall, low CPU) can be told apart from one burning CPU. On platforms
//! without a per-thread CPU clock the CPU fields stay zero.
//!
//! When the environment variable is not set, the only per-span cost is one
//! branch on a cached static flag.

use mudu_sys::time::{instant_now, thread_cpu_time_now, Instant};
use std::cell::Cell;
use std::sync::OnceLock;
use std::time::Duration;

/// Stages instrumented across the query/commit path.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Stage {
    /// `WorkerRuntime::invoke_procedure` total (wasm run + host calls).
    ProcInvoke,
    /// `MuduConnCore::parse_one` (statement parse; cached after first hit).
    SqlParse,
    /// `Binder::bind`.
    SqlBind,
    /// `Planner::plan_query` / `plan_command`.
    SqlPlan,
    /// `cmd.prepare()`.
    SqlPrepare,
    /// `cmd.run()` / `query_exec_to_rows`.
    SqlRun,
    /// `acquire_statement_lock` wait.
    StmtLock,
    /// Whole commit (`worker_commit_tx_with_lock_owner_async`).
    CommitTotal,
    /// `acquire_commit_locks` inside commit.
    CommitLocks,
    /// `storage.prepare_commit_async`.
    CommitPrepare,
    /// WAL serialize + `enqueue_group_commit`.
    WalEnqueue,
    /// `storage.apply_prepared_commit_async`.
    StorageApply,
    /// `drive_group_commit_flush` (inline write+fsync driven by the
    /// committing task, outside the commit locks).
    WalDrive,
    /// `wait_group_commit_advanced` (durability / fsync wait).
    WaitDurable,
    /// `send_partition_rpc` total (cross-worker round trip).
    PartitionRpc,
    /// `dispatch_frame_async` per protocol frame.
    FrameHandle,
    /// Point-read routing: key tuple build + partition routing/worker
    /// resolution + transaction overlay check (`WorkerXContract::_read_key`).
    ReadKeyRoute,
    /// Point-read storage fetch: `storage.get_on_partition` /
    /// `read_visible_relation_value` (`WorkerXContract::_read_key`).
    ReadKeyStorage,
    /// Relation read: `index.get` + `DataRow` lookup (`visible_meta`).
    VisIndexGet,
    /// Relation read: `read_visible_version_async` (version chain walk +
    /// snapshot visibility).
    VisVersionRead,
    /// Relation read: `read_value_payload` (`value_file.get` page chain).
    VisPageRead,
    /// Result row materialization: `tuple_field_to_value`.
    ResultDecode,
    /// UPDATE/INSERT read-old-value segment (storage get or remote
    /// lock-and-read), excluding `acquire_statement_lock`.
    WriteStmtRead,
    /// UPDATE/INSERT write-staging segment (overlay `put_relation` /
    /// `put_on_partition`), excluding `acquire_statement_lock`.
    WriteStmtStaging,
    /// `Relation::write_rows` stripe-lock acquisition (ordered lock of every
    /// stripe the batch touches).
    WrStripeWait,
    /// `Relation::write_rows` per-row `DataRow` / tuple id resolution.
    WrResolve,
    /// `TimeSeriesFile::insert_batch` `write_latch` acquisition.
    WrFileLatch,
    /// Per-row `find_insert_location` page-chain walk inside
    /// `plan_row_insert`.
    WrLocate,
    /// Per-row page mutation inside `plan_row_insert` (insert/update/split/
    /// link fixes), excluding the location walk.
    WrPageOp,
    /// `persist_plan` PL WAL build + append.
    WrWalAppend,
    /// `persist_plan` plan application (page-cache publish + metadata store
    /// + threshold flush check).
    WrPublish,
    /// `Relation::write_rows` final per-row `write_shallow` + `index.insert`.
    WrRowIndex,
    /// WAL flush round write completion wait (io_uring write handles or
    /// tokio `write_all_at`) inside `execute_flush_batch`.
    WalWriteWait,
    /// WAL flush round fsync completion wait (io_uring flush handles or
    /// tokio `fsync`) inside `execute_flush_batch`: device time plus the
    /// event-loop completion-pickup delay.
    WalFsyncWait,
    /// One `execute_flush_batch` round; the count is the fsync-round rate.
    FlushRound,
    /// One `WorkerStorage::flush_dirty_pages_async` sweep over all local
    /// relations.
    PageFlush,
    /// `build_pl_batch` page-diff segment of `persist_plan`.
    WrWalDiff,
    /// PL batch serialize + page LSN stamp + checksum finalize segment of
    /// `persist_plan`.
    WrWalEncode,
    /// `append_frames_async` WAL queue segment of `persist_plan`.
    WrWalQueue,
    /// Connection task `read_next_frame` wait: for a closed-loop client this
    /// is the response-delivery + client-turnaround + request-transit segment
    /// between two frames of one connection.
    ConnReadWait,
    /// Connection task `write_response` (io_uring send submit + completion).
    RespSendWait,
    /// WAL group-commit queueing: batch enqueue to being drained into a
    /// flush round.
    WalQueueWait,
    /// WAL watermark advance to the durability waiter actually resuming.
    WalWakeLag,
    /// Count of commits whose durability wait covers PL frames enqueued
    /// during storage apply (last allocated LSN beyond the XL enqueue).
    CommitWaitPlFrames,
    /// WAL flush round: execute_flush_batch start to all write SQEs
    /// submitted into the ring (file checkout + write_submit loop).
    WalPrepSubmit,
}

const STAGE_COUNT: usize = 45;

const STAGE_NAMES: [&str; STAGE_COUNT] = [
    "proc_invoke",
    "sql_parse",
    "sql_bind",
    "sql_plan",
    "sql_prepare",
    "sql_run",
    "stmt_lock",
    "commit_total",
    "commit_locks",
    "commit_prepare",
    "wal_enqueue",
    "storage_apply",
    "wal_drive",
    "wait_durable",
    "partition_rpc",
    "frame_handle",
    "read_key_route",
    "read_key_storage",
    "vis_index_get",
    "vis_version_read",
    "vis_page_read",
    "result_decode",
    "write_stmt_read",
    "write_stmt_stage",
    "wr_stripe_wait",
    "wr_resolve",
    "wr_file_latch",
    "wr_locate",
    "wr_page_op",
    "wr_wal_append",
    "wr_publish",
    "wr_row_index",
    "wal_write_wait",
    "wal_fsync_wait",
    "flush_round",
    "page_flush",
    "wr_wal_diff",
    "wr_wal_encode",
    "wr_wal_queue",
    "conn_read_wait",
    "resp_send_wait",
    "wal_queue_wait",
    "wal_wake_lag",
    "commit_wait_pl_frames",
    "wal_prep_submit",
];

const DUMP_INTERVAL: Duration = Duration::from_secs(10);

struct StageAccum {
    count: Cell<u64>,
    total_ns: Cell<u64>,
    cpu_total_ns: Cell<u64>,
}

thread_local! {
    static ACCUM: [StageAccum; STAGE_COUNT] = const {
        [const {
            StageAccum {
                count: Cell::new(0),
                total_ns: Cell::new(0),
                cpu_total_ns: Cell::new(0),
            }
        }; STAGE_COUNT]
    };
    static LAST_DUMP: Cell<Option<Instant>> = const { Cell::new(None) };
}

fn stats_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        mudu_sys::env_var::var("MUDU_STAGE_STATS")
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
    })
}

/// Wall-clock start of a span plus the thread CPU-time reading at entry
/// (`None` where no per-thread CPU clock exists).
type SpanStart = (Instant, Option<Duration>);

/// RAII timer for one stage span. When stats are disabled this is a
/// zero-cost marker (one branch on a cached flag at construction and drop).
pub(crate) struct StageGuard {
    stage: Stage,
    start: Option<SpanStart>,
}

impl StageGuard {
    pub(crate) fn new(stage: Stage) -> Self {
        Self {
            stage,
            start: if stats_enabled() {
                Some((instant_now(), thread_cpu_time_now()))
            } else {
                None
            },
        }
    }
}

impl Drop for StageGuard {
    fn drop(&mut self) {
        let Some((start, cpu_start)) = self.start.take() else {
            return;
        };
        let elapsed_ns = start.elapsed().as_nanos() as u64;
        let cpu_ns = cpu_start
            .and_then(|begin| thread_cpu_time_now().map(|end| end.saturating_sub(begin)))
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or(0);
        record(self.stage, elapsed_ns, cpu_ns);
    }
}

fn record(stage: Stage, wall_ns: u64, cpu_ns: u64) {
    ACCUM.with(|accum| {
        let slot = &accum[stage as usize];
        slot.count.set(slot.count.get() + 1);
        slot.total_ns.set(slot.total_ns.get() + wall_ns);
        slot.cpu_total_ns.set(slot.cpu_total_ns.get() + cpu_ns);
    });
}

/// Records a pre-computed value (not a guard-measured span) into a stage,
/// e.g. queueing delay derived from a stored timestamp. No-op when stats
/// are disabled. CPU time is recorded as zero.
pub(crate) fn record_value(stage: Stage, wall_ns: u64) {
    if !stats_enabled() {
        return;
    }
    record(stage, wall_ns, 0);
}

/// Logs the aggregated stage counters (one line per worker thread) and
/// resets them, at most once every [`DUMP_INTERVAL`]. Intended to be called
/// from each worker's event-loop iteration; it is a no-op when stats are
/// disabled or the interval has not elapsed.
pub(crate) fn dump_if_due(worker_id: u128) {
    if !stats_enabled() {
        return;
    }
    let now = instant_now();
    let due = LAST_DUMP.with(|last| match last.get() {
        Some(previous) => previous.elapsed() >= DUMP_INTERVAL,
        None => true,
    });
    if !due {
        return;
    }
    LAST_DUMP.with(|last| last.set(Some(now)));
    ACCUM.with(|accum| {
        let mut line = String::new();
        for (index, name) in STAGE_NAMES.iter().enumerate() {
            let count = accum[index].count.replace(0);
            let total_ns = accum[index].total_ns.replace(0);
            let cpu_total_ns = accum[index].cpu_total_ns.replace(0);
            if count == 0 {
                continue;
            }
            let total_us = total_ns as f64 / 1_000.0;
            let avg_us = total_us / count as f64;
            let cpu_total_us = cpu_total_ns as f64 / 1_000.0;
            let cpu_avg_us = cpu_total_us / count as f64;
            line.push_str(&format!(
                " {name}={count}/{total_us:.0}us/{avg_us:.1}us\
                 /{cpu_total_us:.0}us/{cpu_avg_us:.1}us"
            ));
        }
        if !line.is_empty() {
            tracing::info!(worker_id, "stage_stats:{line}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a guard that records regardless of the cached
    /// `MUDU_STAGE_STATS` flag, so tests do not depend on the environment.
    fn recording_guard(stage: Stage) -> StageGuard {
        StageGuard {
            stage,
            start: Some((instant_now(), thread_cpu_time_now())),
        }
    }

    fn slot_values(stage: Stage) -> (u64, u64, u64) {
        ACCUM.with(|accum| {
            let slot = &accum[stage as usize];
            (
                slot.count.get(),
                slot.total_ns.get(),
                slot.cpu_total_ns.get(),
            )
        })
    }

    #[test]
    fn disabled_guard_records_nothing() {
        // `MUDU_STAGE_STATS` is unset in the test environment, so the cached
        // flag is false and the guard is a zero-cost marker.
        assert!(!stats_enabled());
        let guard = StageGuard::new(Stage::SqlParse);
        assert!(guard.start.is_none());
        drop(guard);
        assert_eq!(slot_values(Stage::SqlParse), (0, 0, 0));
    }

    #[test]
    fn busy_loop_guard_accumulates_cpu_close_to_wall() {
        let guard = recording_guard(Stage::SqlRun);
        // Spin on the thread CPU clock, not wall time: on a contended CI
        // runner the kernel may deschedule the spinner for much of a wall
        // window, but the CPU clock only advances while the thread runs, so
        // this still burns the full 20 ms of CPU (wall simply stretches).
        // Fall back to a wall deadline where no per-thread CPU clock exists.
        match thread_cpu_time_now() {
            Some(cpu_start) => {
                let cpu_deadline = cpu_start + Duration::from_millis(20);
                while thread_cpu_time_now().is_some_and(|now| now < cpu_deadline) {
                    std::hint::spin_loop();
                }
            }
            None => {
                let spin_deadline = instant_now() + Duration::from_millis(20);
                while instant_now() < spin_deadline {
                    std::hint::spin_loop();
                }
            }
        }
        drop(guard);

        let (count, wall_ns, cpu_ns) = slot_values(Stage::SqlRun);
        assert_eq!(count, 1);
        assert!(wall_ns >= 20_000_000, "wall must cover the spin: {wall_ns}");
        if thread_cpu_time_now().is_some() {
            assert!(cpu_ns > 0, "a busy loop must consume cpu time");
            assert!(
                cpu_ns <= wall_ns,
                "thread cpu cannot exceed wall for a single-threaded span: \
                 cpu={cpu_ns} wall={wall_ns}"
            );
            // Loose bound (symmetric with `sleeping_guard_accumulates_little_cpu`,
            // which expects cpu * 4 < wall): even under heavy preemption a busy
            // loop keeps a clear majority of its time on-CPU.
            assert!(
                cpu_ns * 4 >= wall_ns,
                "a busy loop is mostly cpu: cpu={cpu_ns} wall={wall_ns}"
            );
        } else {
            assert_eq!(cpu_ns, 0, "no per-thread cpu clock means zero cpu");
        }
    }

    #[test]
    fn sleeping_guard_accumulates_little_cpu() {
        let guard = recording_guard(Stage::WaitDurable);
        mudu_sys::task::sync::sleep_blocking(Duration::from_millis(50));
        drop(guard);

        let (count, wall_ns, cpu_ns) = slot_values(Stage::WaitDurable);
        assert_eq!(count, 1);
        assert!(
            wall_ns >= 50_000_000,
            "wall must cover the sleep: {wall_ns}"
        );
        if thread_cpu_time_now().is_some() {
            assert!(
                cpu_ns * 4 < wall_ns,
                "sleeping is mostly waiting, not cpu: cpu={cpu_ns} wall={wall_ns}"
            );
        } else {
            assert_eq!(cpu_ns, 0, "no per-thread cpu clock means zero cpu");
        }
    }
}
