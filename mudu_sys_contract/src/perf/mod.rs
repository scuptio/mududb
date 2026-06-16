use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

/// Transaction processing stages that can be measured end-to-end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TxnStage {
    // Client side
    ConnectionEstablish = 0,
    ConnectionPoolWait = 1,
    ClientSerialize = 2,
    ClientNetworkSend = 3,
    ClientNetworkRecv = 4,
    ClientDeserialize = 5,

    // Network / queueing
    ServerQueue = 6,

    // Server side
    NetworkRecv = 7,
    Parse = 8,
    Plan = 9,
    QueryExec = 10,
    CommandExec = 11,
    ProcedureExec = 12,
    TxnLock = 13,
    Commit = 14,
    WriteLog = 15,
    NetworkSend = 16,

    // Whole transaction wall-clock
    Total = 17,

    // Must be last; used to size fixed arrays.
    Count = 18,
}

impl TxnStage {
    #[inline]
    pub const fn as_index(self) -> usize {
        self as usize
    }

    pub const fn all() -> &'static [TxnStage] {
        &[
            TxnStage::ConnectionEstablish,
            TxnStage::ConnectionPoolWait,
            TxnStage::ClientSerialize,
            TxnStage::ClientNetworkSend,
            TxnStage::ClientNetworkRecv,
            TxnStage::ClientDeserialize,
            TxnStage::ServerQueue,
            TxnStage::NetworkRecv,
            TxnStage::Parse,
            TxnStage::Plan,
            TxnStage::QueryExec,
            TxnStage::CommandExec,
            TxnStage::ProcedureExec,
            TxnStage::TxnLock,
            TxnStage::Commit,
            TxnStage::WriteLog,
            TxnStage::NetworkSend,
            TxnStage::Total,
        ]
    }
}

/// Lightweight trace context propagated from client to server.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraceContext {
    pub trace_id: u64,
    pub sampled: bool,
}

impl TraceContext {
    #[inline]
    pub const fn new(trace_id: u64) -> Self {
        Self {
            trace_id,
            sampled: true,
        }
    }

    #[inline]
    pub const fn empty() -> Self {
        Self {
            trace_id: 0,
            sampled: false,
        }
    }
}

/// Aggregated statistics for a single stage.
#[derive(Clone, Copy, Debug, Default)]
pub struct StageBucket {
    pub count: u64,
    pub total_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
}

impl StageBucket {
    #[inline]
    fn add(&mut self, ns: u64) {
        self.count += 1;
        self.total_ns += ns;
        if self.count == 1 || ns < self.min_ns {
            self.min_ns = ns;
        }
        if ns > self.max_ns {
            self.max_ns = ns;
        }
    }

    pub fn avg_ns(&self) -> u64 {
        self.total_ns.checked_div(self.count).unwrap_or(0)
    }
}

/// Per-thread collector. Lock-free and allocation-free.
#[derive(Default)]
pub struct LocalCollector {
    pub buckets: [StageBucket; TxnStage::Count as usize],
    pub sampled_txns: u64,
}

impl LocalCollector {
    #[inline]
    pub fn record(&mut self, stage: TxnStage, ns: u64) {
        self.buckets[stage.as_index()].add(ns);
        if stage == TxnStage::Total {
            self.sampled_txns += 1;
        }
    }

    pub fn merge_into(&self, out: &mut [StageBucket; TxnStage::Count as usize], txns: &mut u64) {
        for (i, bucket) in self.buckets.iter().enumerate() {
            out[i].count += bucket.count;
            out[i].total_ns += bucket.total_ns;
            if bucket.count > 0 {
                if out[i].count == bucket.count || bucket.min_ns < out[i].min_ns {
                    out[i].min_ns = bucket.min_ns;
                }
                if bucket.max_ns > out[i].max_ns {
                    out[i].max_ns = bucket.max_ns;
                }
            }
        }
        *txns += self.sampled_txns;
    }
}

thread_local! {
    static LOCAL: RefCell<LocalCollector> = RefCell::new(LocalCollector::default());
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static SAMPLE_RATE: AtomicU64 = AtomicU64::new(1);
static NEXT_TRACE_ID: AtomicU64 = AtomicU64::new(1);

/// Enable or disable performance collection globally.
#[inline]
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

#[inline]
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Set sampling rate. `1` means sample every transaction; `N` means sample 1/N.
#[inline]
pub fn set_sample_rate(rate: u64) {
    SAMPLE_RATE.store(rate.max(1), Ordering::Relaxed);
}

#[inline]
pub fn sample_rate() -> u64 {
    SAMPLE_RATE.load(Ordering::Relaxed)
}

/// Generate a new unique trace id.
#[inline]
pub fn next_trace_id() -> u64 {
    NEXT_TRACE_ID.fetch_add(1, Ordering::Relaxed)
}

/// Decide whether the current transaction should be sampled.
#[inline]
pub fn should_sample() -> bool {
    if !is_enabled() {
        return false;
    }
    let rate = sample_rate();
    if rate == 1 {
        return true;
    }
    let id = NEXT_TRACE_ID.load(Ordering::Relaxed);
    id.is_multiple_of(rate)
}

/// RAII performance span. Records elapsed time when dropped.
pub struct PerfSpan(Option<PerfSpanInner>);

struct PerfSpanInner {
    stage: TxnStage,
    start: Instant,
}

impl PerfSpan {
    /// Enter a stage. If perf is disabled this is a zero-cost no-op.
    #[inline]
    pub fn new(stage: TxnStage, _trace_id: u64) -> Self {
        if !is_enabled() {
            return Self(None);
        }
        Self(Some(PerfSpanInner {
            stage,
            start: Instant::now(),
        }))
    }

    /// Manually end the span early and record its duration.
    #[inline]
    pub fn finish(mut self) {
        if let Some(inner) = self.0.take() {
            inner.record();
        }
    }
}

impl PerfSpanInner {
    #[inline]
    fn record(self) {
        let dur = self.start.elapsed().as_nanos() as u64;
        LOCAL.with(|c| c.borrow_mut().record(self.stage, dur));
    }
}

impl Drop for PerfSpan {
    #[inline]
    fn drop(&mut self) {
        if let Some(inner) = self.0.take() {
            inner.record();
        }
    }
}

/// Snapshot of aggregated performance metrics.
#[derive(Clone, Debug, Default)]
pub struct PerfSnapshot {
    pub buckets: [StageBucket; TxnStage::Count as usize],
    pub sampled_txns: u64,
}

impl PerfSnapshot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(stage: TxnStage) -> StageBucket {
        LOCAL.with(|c| c.borrow().buckets[stage.as_index()])
    }

    pub fn avg_ns(&self, stage: TxnStage) -> u64 {
        self.buckets[stage.as_index()].avg_ns()
    }
}

/// Take a global snapshot by draining all thread-local collectors.
pub fn snapshot() -> PerfSnapshot {
    // Note: this only snapshots the current thread. To support multi-thread
    // aggregation in production, register each thread's LocalCollector in a
    // global registry and merge them here.
    let mut snap = PerfSnapshot::new();
    LOCAL.with(|c| {
        let local = c.borrow();
        local.merge_into(&mut snap.buckets, &mut snap.sampled_txns);
    });
    snap
}

/// Reset the current thread's collector.
pub fn reset() {
    LOCAL.with(|c| {
        *c.borrow_mut() = LocalCollector::default();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perf_span_records_when_enabled() {
        reset();
        set_enabled(true);
        set_sample_rate(1);

        {
            let _total = PerfSpan::new(TxnStage::Total, 1);
            let _parse = PerfSpan::new(TxnStage::Parse, 1);
            std::thread::sleep(std::time::Duration::from_micros(10));
        }

        let parse = PerfSnapshot::get(TxnStage::Parse);
        assert_eq!(parse.count, 1);
        assert!(parse.total_ns > 0);

        let total = PerfSnapshot::get(TxnStage::Total);
        assert_eq!(total.count, 1);
        assert!(total.total_ns >= parse.total_ns);

        set_enabled(false);
        reset();
    }

    #[test]
    fn perf_span_is_noop_when_disabled() {
        reset();
        set_enabled(false);

        {
            let _ = PerfSpan::new(TxnStage::Parse, 2);
        }

        let parse = PerfSnapshot::get(TxnStage::Parse);
        assert_eq!(parse.count, 0);
    }
}
