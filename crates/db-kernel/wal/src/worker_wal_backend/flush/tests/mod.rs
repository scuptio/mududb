#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::lsn::LSN;
use crate::worker_wal_backend::backend::FLUSH_SLOT_COUNT;
use crate::worker_wal_backend::backend::{WorkerLogInner, WorkerWALBackend};
use crate::worker_wal_backend::batching::WorkerLogBatching;
use crate::worker_wal_backend::layout::{WorkerLogLayout, WorkerLogTail};
use crate::worker_wal_backend::state::ChunkedWorkerLog;
use crate::worker_wal_backend::sync_policy::WalSyncPolicy;
use mudu_sys::default_sys_io_context;
use mudu_sys::env_var::temp_dir;
use mudu_sys::time::instant_now;
use mudu_utils::oid::gen_oid;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::Arc;
use std::time::Duration;

mod basic;
mod group_commit;
mod periodic_fsync;
mod slots;

fn make_batching(
    trigger_bytes: usize,
    trigger_frames: usize,
    max_wait: Duration,
    max_batch_bytes: usize,
) -> EffectiveBatching {
    EffectiveBatching::new(trigger_bytes, trigger_frames, max_wait, max_batch_bytes)
}

fn make_backend_with_queue(queue: Vec<QueuedLogBatch>) -> WorkerWALBackend {
    make_backend_with_queue_and_batching(queue, WorkerLogBatching::default())
}

fn make_backend_with_queue_and_batching(
    queue: Vec<QueuedLogBatch>,
    batching: WorkerLogBatching,
) -> WorkerWALBackend {
    make_backend_with_queue_batching_and_sync_policy(queue, batching, WalSyncPolicy::Commit)
}

fn make_backend_with_queue_batching_and_sync_policy(
    queue: Vec<QueuedLogBatch>,
    batching: WorkerLogBatching,
    sync_policy: WalSyncPolicy,
) -> WorkerWALBackend {
    let dir = temp_dir().join(format!("worker_wal_flush_test_{}", gen_oid()));
    mudu_sys::fs::sync::create_dir_all(&dir).unwrap();
    let layout = WorkerLogLayout::new(dir, gen_oid(), 4096)
        .unwrap()
        .with_batching(batching)
        .with_sync_policy(sync_policy);
    let tail = WorkerLogTail {
        current_sequence: None,
        current_size: 0,
        next_sequence: 0,
        next_lsn: LSN::new(0),
    };
    WorkerWALBackend {
        inner: Arc::new(WorkerLogInner {
            io: default_sys_io_context().provider_arc(),
            log_queue: SMutex::new(queue),
            batching: layout.batching(),
            active_sessions: Arc::new(AtomicUsize::new(0)),
            next_lsn: AtomicU64::new(0),
            flush_waiter: WaitLsn::new(LSN::new(0), vec![], Some(layout.log_oid)),
            flush_trigger: ANotify::new(),
            flush_drivers: std::sync::atomic::AtomicUsize::new(0),
            flush_rounds: AtomicU64::new(0),
            sync_policy: layout.sync_policy(),
            last_fsync: SMutex::new(instant_now().into_std()),
            unsynced_paths: SMutex::new(HashSet::new()),
            state: SMutex::new(ChunkedWorkerLog::new(layout.clone(), tail).unwrap()),
        }),
        flush_tasks: Arc::new((0..FLUSH_SLOT_COUNT).map(|_| SMutex::new(None)).collect()),
        fsync_task: Arc::new(SMutex::new(None)),
    }
}

fn queued_batch(
    frames: usize,
    bytes: usize,
    force_flush: bool,
    elapsed: Duration,
) -> QueuedLogBatch {
    QueuedLogBatch {
        frames: vec![vec![]; frames],
        lsns: vec![],
        bytes,
        enqueued_at: instant_now().into_std() - elapsed,
        force_flush,
    }
}

fn force_queued_batch(lsn: u64) -> QueuedLogBatch {
    QueuedLogBatch {
        frames: vec![vec![1u8; 32]],
        lsns: vec![LSN::new(lsn)],
        bytes: 32,
        enqueued_at: instant_now().into_std(),
        force_flush: true,
    }
}
