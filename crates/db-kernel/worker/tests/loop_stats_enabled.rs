//! The stats-enabled loop-stats paths in a dedicated test process:
//! `MUDU_LOOP_STATS` is cached in a `OnceLock` on first read, so setting it
//! here is deterministic and cannot poison the lib-test binary.

#![allow(clippy::unwrap_used)]

use worker::loop_stats::{count_by, dump_if_due, LoopCounter};

#[test]
fn enabled_loop_stats_count_and_dump() {
    mudu_sys::env_var::set_var("MUDU_LOOP_STATS", "1");
    count_by(LoopCounter::Iterations, 3);
    count_by(LoopCounter::SubmitSqes, 2);
    // First dump is always due; the second is within the interval.
    dump_if_due(3);
    dump_if_due(3);
}
