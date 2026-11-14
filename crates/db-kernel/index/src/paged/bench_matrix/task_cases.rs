//! Identical traces for both indexes at each local-task concurrency level.
use super::{number, Case};

pub(super) fn cases() -> Vec<Case> {
    let keys = number("MUDU_BENCH_KEYS", 4096);
    let levels = mudu_sys::env_var::var("MUDU_BENCH_TASKS").unwrap_or_else(|| "1,4,16".into());
    let mut cases = Vec::new();
    for tasks in levels.split(',') {
        let tasks: usize = tasks.parse().expect("invalid task count");
        assert!(tasks > 0 && tasks <= keys);
        for workload in [
            "read",
            "mixed90",
            "mixed50",
            "changed-update",
            "insert",
            "delete",
            "scan32",
            "scan256",
        ] {
            cases.push(Case {
                name: format!("{workload}-t{tasks}"),
                workload,
                tasks,
                keys,
                key_bytes: number("MUDU_BENCH_KEY_BYTES", 8),
                page_size: number("MUDU_BENCH_PAGE_SIZE", 4096),
                frames: number("MUDU_BENCH_TASK_FRAMES", 4096),
                ..Case::default()
            });
        }
    }
    cases
}
