use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

/// Transaction processing stages that can be measured end-to-end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TxnStage {
    /// Establishing a client connection.
    ConnectionEstablish = 0,
    /// Waiting for a connection from the pool.
    ConnectionPoolWait = 1,
    /// Serializing the client request.
    ClientSerialize = 2,
    /// Sending the request over the network from the client.
    ClientNetworkSend = 3,
    /// Receiving the response over the network at the client.
    ClientNetworkRecv = 4,
    /// Deserializing the client response.
    ClientDeserialize = 5,

    /// Waiting in the server queue.
    ServerQueue = 6,

    /// Receiving the request at the server.
    NetworkRecv = 7,
    /// Parsing the request.
    Parse = 8,
    /// Planning the statement.
    Plan = 9,
    /// Executing a query.
    QueryExec = 10,
    /// Executing a command.
    CommandExec = 11,
    /// Executing a stored procedure.
    ProcedureExec = 12,
    /// Acquiring transaction locks.
    TxnLock = 13,
    /// Committing the transaction.
    Commit = 14,
    /// Writing the transaction log.
    WriteLog = 15,
    /// Sending the response over the network from the server.
    NetworkSend = 16,

    /// Whole transaction wall-clock time.
    Total = 17,

    /// Number of stages; must be last and is used to size fixed arrays.
    Count = 18,
}

impl TxnStage {
    /// Return the zero-based index of this stage.
    #[inline]
    pub const fn as_index(self) -> usize {
        self as usize
    }

    /// Return a slice containing every stage except [`TxnStage::Count`].
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
    /// Unique identifier for this trace.
    pub trace_id: u64,
    /// Whether this trace is being sampled.
    pub sampled: bool,
}

impl TraceContext {
    /// Create a new sampled trace context.
    #[inline]
    pub const fn new(trace_id: u64) -> Self {
        Self {
            trace_id,
            sampled: true,
        }
    }

    /// Return an empty, unsampled trace context.
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
    /// Number of observations.
    pub count: u64,
    /// Total elapsed nanoseconds across all observations.
    pub total_ns: u64,
    /// Minimum observed latency in nanoseconds.
    pub min_ns: u64,
    /// Maximum observed latency in nanoseconds.
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

    /// Return the average latency in nanoseconds.
    pub fn avg_ns(&self) -> u64 {
        self.total_ns.checked_div(self.count).unwrap_or(0)
    }
}

/// Per-thread collector. Lock-free and allocation-free.
#[derive(Default)]
pub struct LocalCollector {
    /// Buckets for each transaction stage.
    pub buckets: [StageBucket; TxnStage::Count as usize],
    /// Number of fully-sampled transactions.
    pub sampled_txns: u64,
}

impl LocalCollector {
    /// Record `ns` nanoseconds for `stage`.
    #[inline]
    pub fn record(&mut self, stage: TxnStage, ns: u64) {
        self.buckets[stage.as_index()].add(ns);
        if stage == TxnStage::Total {
            self.sampled_txns += 1;
        }
    }

    /// Merge this collector's data into `out` and add sampled transaction count to `txns`.
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

/// Return whether performance collection is enabled.
#[inline]
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Set sampling rate. `1` means sample every transaction; `N` means sample 1/N.
#[inline]
pub fn set_sample_rate(rate: u64) {
    SAMPLE_RATE.store(rate.max(1), Ordering::Relaxed);
}

/// Return the current sampling rate.
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
    /// Aggregated stage buckets.
    pub buckets: [StageBucket; TxnStage::Count as usize],
    /// Number of sampled transactions aggregated into this snapshot.
    pub sampled_txns: u64,
}

impl PerfSnapshot {
    /// Create an empty snapshot.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the current bucket for `stage` on this thread.
    pub fn get(stage: TxnStage) -> StageBucket {
        LOCAL.with(|c| c.borrow().buckets[stage.as_index()])
    }

    /// Return the average latency in nanoseconds for `stage`.
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    // The perf subsystem uses process-wide and thread-local mutable state;
    // serialize tests that touch it so parallel execution does not race.
    // mudu_sys_contract cannot depend on mudu_sys, so we use the std mutex here
    // and scope the clippy allowance to this test-only helper.
    #[allow(clippy::disallowed_types)]
    fn perf_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap()
    }

    #[test]
    fn perf_span_records_when_enabled() {
        let _guard = perf_test_lock();
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
        let _guard = perf_test_lock();
        reset();
        set_enabled(false);

        {
            let _ = PerfSpan::new(TxnStage::Parse, 2);
        }

        let parse = PerfSnapshot::get(TxnStage::Parse);
        assert_eq!(parse.count, 0);
    }
}
