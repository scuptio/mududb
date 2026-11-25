//! Per-stream operational metrics of the generic replication mechanism
//! (G-08): cheap in-process counters keyed by group/stream identity,
//! reported to the node's status surface by the owning service.

use mudu_raft::stream::{GroupId, StreamId};
use mudu_sys::sync::SMutex;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

/// Counters of one stream; all monotonic since the process started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamCounters {
    /// Structs durably appended through this node.
    pub appended: u64,
    /// Structs applied by the business machine.
    pub applied: u64,
    /// Chunks reclaimed by prefix GC.
    pub gc_chunks: u64,
}

#[derive(Default)]
struct MetricsInner {
    streams: BTreeMap<(GroupId, StreamId), StreamCounters>,
}

/// Process-wide registry of stream counters. Business identity stays out of
/// the metric keys: group + stream only (bounded cardinality).
pub struct StreamMetrics {
    inner: SMutex<MetricsInner>,
}

fn registry() -> &'static Arc<StreamMetrics> {
    static REGISTRY: OnceLock<Arc<StreamMetrics>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        Arc::new(StreamMetrics {
            inner: SMutex::new(MetricsInner::default()),
        })
    })
}

impl StreamMetrics {
    /// The process-wide registry.
    pub fn global() -> Arc<StreamMetrics> {
        registry().clone()
    }

    /// Structs one durable append.
    pub fn note_append(&self, group: GroupId, stream: StreamId) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.streams.entry((group, stream)).or_default().appended += 1;
        }
    }

    /// Structs one applied record.
    pub fn note_apply(&self, group: GroupId, stream: StreamId) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.streams.entry((group, stream)).or_default().applied += 1;
        }
    }

    /// Structs prefix-GC'd bytes.
    pub fn note_gc(&self, group: GroupId, stream: StreamId, chunks: u64) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.streams.entry((group, stream)).or_default().gc_chunks += chunks;
        }
    }

    /// A consistent snapshot of all counters (status endpoints).
    pub fn snapshot(&self) -> BTreeMap<(GroupId, StreamId), StreamCounters> {
        self.inner
            .lock()
            .map(|inner| inner.streams.clone())
            .unwrap_or_default()
    }
}
