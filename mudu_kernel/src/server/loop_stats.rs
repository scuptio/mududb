//! Lightweight event-loop phase accounting for the io_uring worker loop.
//!
//! When `MUDU_LOOP_STATS=1` is set in the environment, the phases of each
//! `run_service_loop` iteration (mailbox handling, WAL flush/fsync slot
//! polling, task polling, ring submit, CQE wait) are timed with an RAII
//! [`LoopGuard`] and accumulated into thread-local counters, alongside a few
//! counter-only events (iterations, budget-exhausted poll slices, wait
//! timeouts, 1ms idle sleeps, processed CQEs). Each worker event loop
//! periodically calls [`dump_if_due`], which logs the aggregated numbers and
//! resets them.
//!
//! This mirrors the `MUDU_STAGE_STATS` mechanism in
//! [`crate::server::stage_stats`]; when the environment variable is not set,
//! the only per-span cost is one branch on a cached static flag.

use mudu_sys::time::{instant_now, Instant};
use std::cell::Cell;
use std::sync::OnceLock;
use std::time::Duration;

/// Timed phases of one `run_service_loop` iteration.
#[derive(Clone, Copy, Debug)]
pub(crate) enum LoopPhase {
    /// Mailbox drain + message handling.
    Mailbox,
    /// WAL flush/fsync task-slot polling (`poll_flush_log` +
    /// shutdown `force_flush_log`).
    PollFlushLog,
    /// `poll_ready_worker_tasks` bounded poll slice.
    TaskPoll,
    /// `ring.submit()`.
    Submit,
    /// Blocking `wait_for_cqe` (including ETIME/EINTR outcomes).
    WaitCqe,
}

const PHASE_COUNT: usize = 5;

const PHASE_NAMES: [&str; PHASE_COUNT] = [
    "mailbox_msghandling",
    "poll_flush_log",
    "task_poll",
    "submit",
    "wait_cqe",
];

/// Counter-only loop events (no timing).
#[derive(Clone, Copy, Debug)]
pub(crate) enum LoopCounter {
    /// Total loop iterations.
    Iterations,
    /// `poll_ready_worker_tasks` slices that exhausted the poll budget
    /// (returned more_ready=true).
    TaskPollBudgetFull,
    /// `wait_for_cqe` calls that returned -ETIME.
    WaitCqeEtime,
    /// Iterations that took the `sleep_blocking(1ms)` idle path
    /// (inflight empty && !more_ready).
    Sleep1ms,
    /// CQEs processed (direct + drained).
    DrainCqes,
    /// SQEs handed to the kernel by `ring.submit()`.
    SubmitSqes,
}

const COUNTER_COUNT: usize = 6;

const COUNTER_NAMES: [&str; COUNTER_COUNT] = [
    "iterations",
    "task_poll_budget_full",
    "wait_cqe_etime",
    "sleep_1ms",
    "drain_cqes",
    "submit_sqes",
];

const DUMP_INTERVAL: Duration = Duration::from_secs(10);

thread_local! {
    static PHASE_ACCUM: [Cell<u64>; PHASE_COUNT] = const {
        [const { Cell::new(0) }; PHASE_COUNT]
    };
    static PHASE_TOTAL_NS: [Cell<u64>; PHASE_COUNT] = const {
        [const { Cell::new(0) }; PHASE_COUNT]
    };
    static COUNTERS: [Cell<u64>; COUNTER_COUNT] = const {
        [const { Cell::new(0) }; COUNTER_COUNT]
    };
    static LAST_DUMP: Cell<Option<Instant>> = const { Cell::new(None) };
}

fn stats_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        mudu_sys::env_var::var("MUDU_LOOP_STATS")
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
    })
}

/// RAII timer for one loop phase. When stats are disabled this is a
/// zero-cost marker (one branch on a cached flag at construction and drop).
pub(crate) struct LoopGuard {
    phase: LoopPhase,
    start: Option<Instant>,
}

impl LoopGuard {
    pub(crate) fn new(phase: LoopPhase) -> Self {
        Self {
            phase,
            start: if stats_enabled() {
                Some(instant_now())
            } else {
                None
            },
        }
    }
}

impl Drop for LoopGuard {
    fn drop(&mut self) {
        let Some(start) = self.start.take() else {
            return;
        };
        let elapsed_ns = start.elapsed().as_nanos() as u64;
        let index = self.phase as usize;
        PHASE_ACCUM.with(|accum| accum[index].set(accum[index].get() + 1));
        PHASE_TOTAL_NS.with(|totals| totals[index].set(totals[index].get() + elapsed_ns));
    }
}

/// Increments a counter-only loop event by one. One branch on the cached
/// flag when stats are disabled.
pub(crate) fn count(counter: LoopCounter) {
    count_by(counter, 1);
}

/// Increments a counter-only loop event by `n`. One branch on the cached
/// flag when stats are disabled.
pub(crate) fn count_by(counter: LoopCounter, n: u64) {
    if !stats_enabled() {
        return;
    }
    let index = counter as usize;
    COUNTERS.with(|counters| counters[index].set(counters[index].get() + n));
}

/// Logs the aggregated loop counters (one line per worker thread) and
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
    let mut line = String::new();
    COUNTERS.with(|counters| {
        for (index, name) in COUNTER_NAMES.iter().enumerate() {
            let value = counters[index].replace(0);
            if value == 0 {
                continue;
            }
            line.push_str(&format!(" {name}={value}"));
        }
    });
    PHASE_ACCUM.with(|accum| {
        PHASE_TOTAL_NS.with(|totals| {
            for (index, name) in PHASE_NAMES.iter().enumerate() {
                let count = accum[index].replace(0);
                let total_ns = totals[index].replace(0);
                if count == 0 {
                    continue;
                }
                let total_us = total_ns as f64 / 1_000.0;
                let avg_us = total_us / count as f64;
                line.push_str(&format!(" {name}={count}/{total_us:.0}us/{avg_us:.1}us"));
            }
        });
    });
    if !line.is_empty() {
        tracing::info!(worker_id, "loop_stats:{line}");
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    /// Builds a guard that records regardless of the cached
    /// `MUDU_LOOP_STATS` flag, so tests do not depend on the environment.
    fn recording_guard(phase: LoopPhase) -> LoopGuard {
        LoopGuard {
            phase,
            start: Some(instant_now()),
        }
    }

    #[test]
    fn disabled_guard_records_nothing() {
        // `MUDU_LOOP_STATS` is unset in the test environment, so the cached
        // flag is false and the guard is a zero-cost marker.
        assert!(!stats_enabled());
        let guard = LoopGuard::new(LoopPhase::Submit);
        assert!(guard.start.is_none());
        drop(guard);
        let index = LoopPhase::Submit as usize;
        PHASE_ACCUM.with(|accum| assert_eq!(accum[index].get(), 0));
        PHASE_TOTAL_NS.with(|totals| assert_eq!(totals[index].get(), 0));
    }

    #[test]
    fn recording_guard_accumulates_count_and_time() {
        let index = LoopPhase::WaitCqe as usize;
        let (before_count, before_ns) =
            PHASE_ACCUM.with(|accum| (accum[index].get(), PHASE_TOTAL_NS.with(|t| t[index].get())));
        {
            let _guard = recording_guard(LoopPhase::WaitCqe);
            mudu_sys::task::sync::sleep_blocking(Duration::from_millis(5));
        }
        let (after_count, after_ns) =
            PHASE_ACCUM.with(|accum| (accum[index].get(), PHASE_TOTAL_NS.with(|t| t[index].get())));
        assert_eq!(after_count, before_count + 1);
        assert!(
            after_ns >= before_ns + 5_000_000,
            "timed span must cover the sleep: before={before_ns} after={after_ns}"
        );
    }

    #[test]
    fn disabled_count_records_nothing() {
        let index = LoopCounter::Iterations as usize;
        let before = COUNTERS.with(|counters| counters[index].get());
        count(LoopCounter::Iterations);
        count_by(LoopCounter::Iterations, 5);
        let after = COUNTERS.with(|counters| counters[index].get());
        assert_eq!(after, before, "disabled stats must not record");
    }
}
