//! Optional per-tree diagnostic counters. Disabled trees perform no counter RMWs.
use super::PagedBTree;
use mudu_sys::time::{instant_now, Instant};
use page_buffer::PreparedEdit;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

/// Cumulative work counters, including failed/retried preparation when applicable.
#[derive(Clone, Copy, Debug, Default)]
pub struct PerformanceStats {
    /// Point-operation revision conflicts or split-latch conflicts requiring retry.
    /// An initial resident-path fallback is not a retry; maintenance is excluded.
    pub retries: u64,
    /// Page bytes copied to preserve a shared immutable snapshot during edits.
    pub cow_bytes: u64,
    /// Bytes moved by published packed-page copy-within operations.
    pub moved_bytes: u64,
    /// Logical overflow page visits, including buffer hits (not physical I/O).
    pub overflow_reads: u64,
    /// Resident page latch attempts that could not complete immediately.
    pub latch_waits: u64,
    /// Nanoseconds resolving resident latch contention, including bounded probes.
    /// Cancelled waits are excluded.
    pub latch_wait_ns: u64,
    /// Contended acquisitions resolved by bounded probes before async queueing.
    pub latch_probe_hits: u64,
    /// Completed checksum computations, including failed integrity checks and writeback retries.
    pub checksum_calls: u64,
    /// Bytes hashed; excludes the eight-byte checksum field, includes page free space.
    pub checksum_bytes: u64,
    /// Elapsed nanoseconds in checksum computation, excluding copying and layout validation.
    pub checksum_ns: u64,
    /// Bytes copied by the index writeback encoder, excluding COW and backing-store copies.
    pub writeback_copy_bytes: u64,
}
#[derive(Debug, Default)]
pub(super) struct Diagnostics {
    retries: AtomicU64,
    cow_bytes: AtomicU64,
    moved_bytes: AtomicU64,
    overflow_reads: AtomicU64,
    latch_waits: AtomicU64,
    latch_wait_ns: AtomicU64,
    latch_probe_hits: AtomicU64,
    checksum_calls: AtomicU64,
    checksum_bytes: AtomicU64,
    checksum_ns: AtomicU64,
    writeback_copy_bytes: AtomicU64,
}
impl<V: Send> PagedBTree<V> {
    /// Enables cumulative diagnostic counters. Leave disabled for normal operation;
    /// concurrent snapshots of counters are approximate, not transactional.
    pub fn with_diagnostics(mut self) -> Self {
        // The owned builder is the only initializer. Flushes only use OnceLock::get,
        // which never waits; existing image policies observe activation as well.
        if self.diagnostics.is_none() {
            let counters = Arc::new(Diagnostics::default());
            let _ = self.writeback.diagnostics.set(counters.clone());
            self.diagnostics = Some(counters);
        }
        self
    }
    /// Returns diagnostic counters, or zeroes when diagnostics are disabled.
    pub fn performance_stats(&self) -> PerformanceStats {
        let Some(d) = &self.diagnostics else {
            return PerformanceStats::default();
        };
        PerformanceStats {
            retries: d.retries.load(Ordering::Relaxed),
            cow_bytes: d.cow_bytes.load(Ordering::Relaxed),
            moved_bytes: d.moved_bytes.load(Ordering::Relaxed),
            overflow_reads: d.overflow_reads.load(Ordering::Relaxed),
            latch_waits: d.latch_waits.load(Ordering::Relaxed),
            latch_wait_ns: d.latch_wait_ns.load(Ordering::Relaxed),
            latch_probe_hits: d.latch_probe_hits.load(Ordering::Relaxed),
            checksum_calls: d.checksum_calls.load(Ordering::Relaxed),
            checksum_bytes: d.checksum_bytes.load(Ordering::Relaxed),
            checksum_ns: d.checksum_ns.load(Ordering::Relaxed),
            writeback_copy_bytes: d.writeback_copy_bytes.load(Ordering::Relaxed),
        }
    }
    /// Clock reads and counters are entirely disabled on normal throughput runs.
    pub(super) fn latch_wait_started(&self) -> Option<Instant> {
        self.diagnostics.as_ref().map(|d| {
            d.latch_waits.fetch_add(1, Ordering::Relaxed);
            instant_now()
        })
    }
    pub(super) fn latch_wait_finished(&self, started: Option<Instant>) {
        if let (Some(d), Some(started)) = (&self.diagnostics, started) {
            let nanos = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
            d.latch_wait_ns.fetch_add(nanos, Ordering::Relaxed);
        }
    }
    pub(super) fn retry(&self) {
        if let Some(d) = &self.diagnostics {
            d.retries.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub(super) fn overflow_read(&self) {
        if let Some(d) = &self.diagnostics {
            d.overflow_reads.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub(super) fn record_edit(&self, write: &PreparedEdit<'_>) {
        if let Some(d) = &self.diagnostics {
            d.cow_bytes
                .fetch_add(write.copied_bytes() as u64, Ordering::Relaxed);
            d.moved_bytes
                .fetch_add(write.moved_bytes() as u64, Ordering::Relaxed);
        }
    }
}

impl Diagnostics {
    pub(super) fn record_checksum(&self, bytes: usize, nanos: u64) {
        self.checksum_calls.fetch_add(1, Ordering::Relaxed);
        self.checksum_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
        self.checksum_ns.fetch_add(nanos, Ordering::Relaxed);
    }
    pub(super) fn record_writeback_copy(&self, bytes: usize) {
        self.writeback_copy_bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }
}
