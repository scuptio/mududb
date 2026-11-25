//! Lifetime operational counters of the replication layer.

use mudu::channel::ServerInstanceId;
use mudu_sys::sync::SMutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

/// Operational counters of the replication layer (this server's lifetime).
#[derive(Clone, Debug, Default)]
pub struct ReplicationCounters {
    /// Synchronous stream commits that failed on the StreamFollower ack timeout.
    pub sync_write_timeouts: u64,
    /// Worker snapshots taken locally.
    pub snapshots_taken: u64,
    /// Worker WAL chunks reclaimed by the prefix GC.
    pub wal_chunks_reclaimed: u64,
    /// Snapshot offers served to lagging subscribers.
    pub snapshot_offers_served: u64,
    /// Snapshot restores completed on this node.
    pub snapshot_restores_completed: u64,
}

pub(crate) fn counter_slot(server_instance_id: ServerInstanceId) -> Arc<ReplicationCountersAtomic> {
    static SLOTS: OnceLock<SMutex<HashMap<ServerInstanceId, Arc<ReplicationCountersAtomic>>>> =
        OnceLock::new();
    let slots = SLOTS.get_or_init(|| SMutex::new(HashMap::new()));
    match slots.lock() {
        Ok(mut map) => map.entry(server_instance_id).or_default().clone(),
        Err(_) => Arc::new(ReplicationCountersAtomic::default()),
    }
}

#[derive(Default)]
pub(crate) struct ReplicationCountersAtomic {
    sync_write_timeouts: AtomicU64,
    snapshots_taken: AtomicU64,
    wal_chunks_reclaimed: AtomicU64,
}

impl ReplicationCountersAtomic {
    pub(crate) fn snapshot(&self) -> ReplicationCounters {
        ReplicationCounters {
            sync_write_timeouts: self.sync_write_timeouts.load(Ordering::Relaxed),
            snapshots_taken: self.snapshots_taken.load(Ordering::Relaxed),
            wal_chunks_reclaimed: self.wal_chunks_reclaimed.load(Ordering::Relaxed),
            snapshot_offers_served: crate::test_hook_snapshot_offers_served(),
            snapshot_restores_completed: crate::test_hook_snapshot_restores_completed(),
        }
    }
}

/// Counter bump: a synchronous stream commit hit the StreamFollower ack timeout.
pub(crate) fn note_sync_write_timeout(server_instance_id: ServerInstanceId) {
    counter_slot(server_instance_id)
        .sync_write_timeouts
        .fetch_add(1, Ordering::Relaxed);
}

/// Counter bump: a worker snapshot was taken on this server.
pub(crate) fn note_snapshot_taken(server_instance_id: ServerInstanceId) {
    counter_slot(server_instance_id)
        .snapshots_taken
        .fetch_add(1, Ordering::Relaxed);
}

/// Counts worker WAL chunks reclaimed by the prefix GC.
pub(crate) fn note_wal_chunks_reclaimed(server_instance_id: ServerInstanceId, chunks: u64) {
    counter_slot(server_instance_id)
        .wal_chunks_reclaimed
        .fetch_add(chunks, Ordering::Relaxed);
}
