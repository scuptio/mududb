//! Fixed dataset and traces across capacities, with a full warmup before timing.
use super::{number, Case};

pub(super) fn enabled() -> bool {
    mudu_sys::env_var::var("MUDU_BENCH_SUITE").as_deref() == Some("cache")
}

pub(super) fn cases() -> Vec<Case> {
    let keys = number("MUDU_BENCH_KEYS", 4096);
    let key_bytes = number("MUDU_BENCH_KEY_BYTES", 8);
    let page_size = number("MUDU_BENCH_PAGE_SIZE", 4096);
    assert!(keys > 256 && key_bytes >= 8);
    let frames =
        mudu_sys::env_var::var("MUDU_BENCH_FRAMES").unwrap_or_else(|| "4096,256,64,16".into());
    let mut cases = Vec::new();
    for workload in ["read", "scan32", "scan256", "delete"] {
        for capacity in frames.split(',') {
            let frames: usize = capacity.parse().expect("invalid frame capacity");
            assert!(frames > 0);
            cases.push(Case {
                name: format!("cache-{workload}-f{frames}"),
                workload,
                keys,
                key_bytes,
                page_size,
                frames,
                ..Case::default()
            });
        }
    }
    cases
}

/// Emit exact operation counts even for the regular matrix's diagnostic reports.
pub(super) fn report(
    case: &Case,
    trial: usize,
    paged: bool,
    pages: u64,
    operations: usize,
    before: super::BufferStats,
    after: super::BufferStats,
) {
    println!(
        "CACHE,{},{},{},{},{}",
        case.name,
        if paged { "paged" } else { "baseline" },
        trial,
        pages,
        operations
    );
    if enabled() && paged && case.frames as u64 >= pages {
        assert_eq!(
            after.reads - before.reads,
            0,
            "full-resident workload loaded a page"
        );
        assert_eq!(
            after.evictions - before.evictions,
            0,
            "full-resident workload evicted a page"
        );
    }
}
