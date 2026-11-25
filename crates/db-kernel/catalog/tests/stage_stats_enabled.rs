//! The stats-enabled stage-stats paths in a dedicated test process:
//! `MUDU_STAGE_STATS` is cached in a `OnceLock` on first read, so setting it
//! here (before any catalog test touches it) is deterministic and cannot
//! poison the lib-test binary.

#![allow(clippy::unwrap_used)]

use catalog::stage_stats::{dump_if_due, record_value, Stage, StageGuard};

#[test]
fn enabled_stats_record_and_dump() {
    mudu_sys::env_var::set_var("MUDU_STAGE_STATS", "1");
    {
        let _guard = StageGuard::new(Stage::WaitDurable);
    }
    record_value(Stage::SqlBind, 1_000);
    // First call is always due: builds and logs the per-stage line.
    dump_if_due(7);
    // Second call within the interval is a no-op.
    dump_if_due(7);
}
