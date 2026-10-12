#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::io::{collect_flush_paths, merge_reserved_writes};
use super::*;
use crate::worker_wal_backend::state::AppendReservation;
use std::cmp::Reverse;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

#[test]
fn wait_lsn_empty_input_leaves_next_wait_lsn_unchanged() {
    let waiter = WaitLsn::new(LSN::new(5), vec![], None);
    waiter.ready(vec![]).unwrap();
    assert_eq!(waiter.next_wait_lsn.load(Ordering::Acquire), 5);
    assert!(waiter.ready_lsns.lock().unwrap().is_empty());
}

#[test]
fn wait_lsn_out_of_order_contiguous_lsns_advance_and_clear() {
    let waiter = WaitLsn::new(LSN::new(5), vec![], None);
    waiter
        .ready(vec![LSN::new(7), LSN::new(5), LSN::new(6)])
        .unwrap();
    assert_eq!(waiter.next_wait_lsn.load(Ordering::Acquire), 8);
    assert!(waiter.ready_lsns.lock().unwrap().is_empty());
}

#[test]
fn wait_lsn_non_contiguous_lsns_advance_to_next_contiguous_and_keep_gaps() {
    let waiter = WaitLsn::new(LSN::new(5), vec![], None);
    waiter.ready(vec![LSN::new(5), LSN::new(7)]).unwrap();
    assert_eq!(waiter.next_wait_lsn.load(Ordering::Acquire), 6);
    let ready_lsns = waiter.ready_lsns.lock().unwrap();
    assert_eq!(ready_lsns.len(), 1);
    assert_eq!(ready_lsns.peek(), Some(&Reverse(7)));
}

#[test]
fn wait_lsn_duplicates_are_deduplicated() {
    let waiter = WaitLsn::new(LSN::new(5), vec![], None);
    waiter
        .ready(vec![LSN::new(5), LSN::new(5), LSN::new(6)])
        .unwrap();
    assert_eq!(waiter.next_wait_lsn.load(Ordering::Acquire), 7);
    assert!(waiter.ready_lsns.lock().unwrap().is_empty());
}

#[test]
fn should_start_flush_empty_queue_returns_false() {
    let batching = make_batching(64, 4, Duration::from_millis(10), 256);
    assert!(!WorkerWALBackend::should_start_flush(&[], batching));
}

#[test]
fn should_start_flush_pending_bytes_threshold_returns_true() {
    let batching = make_batching(64, 4, Duration::from_millis(10), 256);
    let queue = vec![queued_batch(1, 64, false, Duration::ZERO)];
    assert!(WorkerWALBackend::should_start_flush(&queue, batching));
}

#[test]
fn should_start_flush_pending_frames_threshold_returns_true() {
    let batching = make_batching(64, 4, Duration::from_millis(10), 256);
    let queue = vec![queued_batch(4, 16, false, Duration::ZERO)];
    assert!(WorkerWALBackend::should_start_flush(&queue, batching));
}

#[test]
fn should_start_flush_force_flush_returns_true_below_thresholds() {
    let batching = make_batching(64, 4, Duration::from_millis(10), 256);
    let queue = vec![queued_batch(1, 1, true, Duration::ZERO)];
    assert!(WorkerWALBackend::should_start_flush(&queue, batching));
}

#[test]
fn should_start_flush_oldest_batch_expired_returns_true() {
    let batching = make_batching(64, 4, Duration::from_millis(10), 256);
    let queue = vec![queued_batch(
        1,
        1,
        false,
        batching.max_wait + Duration::from_millis(1),
    )];
    assert!(WorkerWALBackend::should_start_flush(&queue, batching));
}

#[test]
fn should_start_flush_below_thresholds_returns_false() {
    let batching = make_batching(64, 4, Duration::from_millis(10), 256);
    let queue = vec![queued_batch(1, 1, false, Duration::ZERO)];
    assert!(!WorkerWALBackend::should_start_flush(&queue, batching));
}

#[test]
fn merge_reserved_writes_empty_returns_empty() {
    assert!(merge_reserved_writes(&[], &[]).is_empty());
}

#[test]
fn merge_reserved_writes_contiguous_same_file_merged() {
    let path = PathBuf::from("/tmp/wal/0.xl");
    let reservations = vec![
        AppendReservation {
            path: path.clone(),
            offset: 0,
            flush_after_write: false,
        },
        AppendReservation {
            path: path.clone(),
            offset: 10,
            flush_after_write: false,
        },
    ];
    let payload = vec![vec![1u8; 10], vec![2u8; 5]];
    let merged = merge_reserved_writes(&reservations, &payload);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].path, path);
    assert_eq!(merged[0].offset, 0);
    assert_eq!(
        merged[0].payload,
        vec![1u8; 10]
            .into_iter()
            .chain(vec![2u8; 5])
            .collect::<Vec<_>>()
    );
}

#[test]
fn merge_reserved_writes_non_contiguous_same_file_separate() {
    let path = PathBuf::from("/tmp/wal/0.xl");
    let reservations = vec![
        AppendReservation {
            path: path.clone(),
            offset: 0,
            flush_after_write: false,
        },
        AppendReservation {
            path: path.clone(),
            offset: 20,
            flush_after_write: false,
        },
    ];
    let payload = vec![vec![1u8; 10], vec![2u8; 5]];
    let merged = merge_reserved_writes(&reservations, &payload);
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].offset, 0);
    assert_eq!(merged[1].offset, 20);
}

#[test]
fn merge_reserved_writes_different_files_separate() {
    let path_a = PathBuf::from("/tmp/wal/0.xl");
    let path_b = PathBuf::from("/tmp/wal/1.xl");
    let reservations = vec![
        AppendReservation {
            path: path_a.clone(),
            offset: 0,
            flush_after_write: false,
        },
        AppendReservation {
            path: path_b.clone(),
            offset: 0,
            flush_after_write: false,
        },
    ];
    let payload = vec![vec![1u8; 10], vec![2u8; 5]];
    let merged = merge_reserved_writes(&reservations, &payload);
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].path, path_a);
    assert_eq!(merged[1].path, path_b);
}

#[test]
fn collect_flush_paths_empty_returns_empty() {
    assert!(collect_flush_paths(&[]).is_empty());
}

#[test]
fn collect_flush_paths_preserves_order_and_deduplicates() {
    let a = PathBuf::from("/tmp/wal/a.xl");
    let b = PathBuf::from("/tmp/wal/b.xl");
    let c = PathBuf::from("/tmp/wal/c.xl");
    let reservations = vec![
        AppendReservation {
            path: a.clone(),
            offset: 0,
            flush_after_write: false,
        },
        AppendReservation {
            path: b.clone(),
            offset: 0,
            flush_after_write: false,
        },
        AppendReservation {
            path: a.clone(),
            offset: 100,
            flush_after_write: false,
        },
        AppendReservation {
            path: c.clone(),
            offset: 0,
            flush_after_write: false,
        },
    ];
    let paths = collect_flush_paths(&reservations);
    assert_eq!(paths, vec![a, b, c]);
}

#[test]
fn next_flush_deadline_no_queued_batch_returns_none() {
    let backend = make_backend_with_queue(vec![]);
    assert!(backend.next_flush_deadline().unwrap().is_none());
}

#[test]
fn next_flush_deadline_all_flush_slots_busy_returns_none() {
    let backend = make_backend_with_queue(vec![queued_batch(1, 1, false, Duration::ZERO)]);
    for slot in backend.flush_tasks.iter() {
        *slot.lock().unwrap() = Some(Box::pin(async move { Ok(()) }));
    }
    assert!(backend.next_flush_deadline().unwrap().is_none());
    // With one slot free, the deadline is computed from the queue again.
    backend.flush_tasks[0].lock().unwrap().take();
    assert!(backend.next_flush_deadline().unwrap().is_some());
}

#[test]
fn next_flush_deadline_thresholds_met_returns_some_not_in_future() {
    let backend = make_backend_with_queue(vec![queued_batch(4, 64, false, Duration::ZERO)]);
    let deadline = backend.next_flush_deadline().unwrap().unwrap();
    assert!(deadline <= instant_now().into_std() + Duration::from_millis(50));
}

#[test]
fn next_flush_deadline_otherwise_returns_oldest_enqueue_plus_max_wait() {
    // Use a long max_wait so the batch cannot age past the threshold between
    // creation and the deadline query, which would make next_flush_deadline
    // return the current instant instead of oldest + max_wait.
    let max_wait = Duration::from_secs(60);
    let batching = WorkerLogBatching::new(64 * 1024, 32, max_wait, 256 * 1024);
    let backend = make_backend_with_queue_and_batching(
        vec![queued_batch(1, 1, false, Duration::ZERO)],
        batching,
    );
    let deadline = backend.next_flush_deadline().unwrap().unwrap();
    let oldest = backend.inner.log_queue.lock().unwrap()[0].enqueued_at;
    assert_eq!(deadline, oldest + max_wait);
}

#[test]
fn drain_pending_batches_respects_max_batch_bytes_by_splitting() {
    let batching = make_batching(64, 4, Duration::from_millis(10), 150);
    let backend = make_backend_with_queue(vec![
        queued_batch(1, 100, false, Duration::ZERO),
        queued_batch(1, 100, false, Duration::ZERO),
    ]);
    let drained = backend.drain_pending_batches(batching).unwrap();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].bytes, 100);
    assert_eq!(backend.inner.log_queue.lock().unwrap().len(), 1);
}

#[test]
fn drain_pending_batches_never_returns_empty_when_queue_non_empty() {
    let batching = make_batching(64, 4, Duration::from_millis(10), 50);
    let backend = make_backend_with_queue(vec![queued_batch(1, 100, false, Duration::ZERO)]);
    let drained = backend.drain_pending_batches(batching).unwrap();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].bytes, 100);
}

#[test]
fn effective_batching_new_preserves_constructor_values() {
    let batching = EffectiveBatching::new(1024, 16, Duration::from_millis(50), 2048);
    assert_eq!(batching.trigger_bytes, 1024);
    assert_eq!(batching.trigger_frames, 16);
    assert_eq!(batching.max_wait, Duration::from_millis(50));
    assert_eq!(batching.max_batch_bytes, 2048);
}

#[test]
fn prepare_flush_batch_merges_frames_and_lsns() {
    let backend = make_backend_with_queue(vec![]);
    let reservations = backend
        .reserve_appends(&[vec![1u8; 10], vec![2u8; 5]])
        .unwrap();
    let pending = vec![QueuedLogBatch {
        frames: vec![vec![1u8; 10], vec![2u8; 5]],
        lsns: vec![LSN::new(1), LSN::new(2)],
        bytes: 15,
        enqueued_at: instant_now().into_std(),
        force_flush: false,
    }];
    let prepared = backend.prepare_flush_batch(pending).unwrap();
    assert_eq!(prepared.writes.len(), 1);
    assert_eq!(prepared.ready_lsns, vec![LSN::new(1), LSN::new(2)]);
    assert_eq!(prepared.writes[0].payload.len(), 15);
    assert_eq!(prepared.flush_paths, collect_flush_paths(&reservations));
}

#[test]
fn complete_persisted_lsns_advances_wait_lsn() {
    let backend = make_backend_with_queue(vec![]);
    backend
        .complete_persisted_lsns(vec![LSN::new(0), LSN::new(1)])
        .unwrap();
    assert_eq!(
        backend
            .inner
            .flush_waiter
            .next_wait_lsn
            .load(Ordering::Acquire),
        2
    );
}

#[test]
fn complete_persisted_lsns_empty_is_noop() {
    let backend = make_backend_with_queue(vec![]);
    backend.complete_persisted_lsns(vec![]).unwrap();
    assert_eq!(
        backend
            .inner
            .flush_waiter
            .next_wait_lsn
            .load(Ordering::Acquire),
        0
    );
}

#[test]
fn poll_flush_log_returns_true_when_queue_is_non_empty() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue(vec![QueuedLogBatch {
            frames: vec![vec![1u8; 64]],
            lsns: vec![LSN::new(0)],
            bytes: 64,
            enqueued_at: instant_now().into_std(),
            force_flush: true,
        }]);
        assert!(backend.poll_flush_log().unwrap());
    })
    .unwrap();
}

#[test]
fn poll_flush_log_returns_false_when_queue_is_empty() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue(vec![]);
        assert!(!backend.poll_flush_log().unwrap());
    })
    .unwrap();
}

#[test]
fn force_flush_log_returns_true_for_non_empty_queue() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue(vec![QueuedLogBatch {
            frames: vec![vec![1u8; 8]],
            lsns: vec![LSN::new(0)],
            bytes: 8,
            enqueued_at: instant_now().into_std(),
            force_flush: false,
        }]);
        assert!(backend.force_flush_log().unwrap());
    })
    .unwrap();
}

#[test]
fn run_flush_log_persists_queued_frames_to_disk() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue(vec![QueuedLogBatch {
            frames: vec![vec![1u8; 32], vec![2u8; 32]],
            lsns: vec![LSN::new(0), LSN::new(1)],
            bytes: 64,
            enqueued_at: instant_now().into_std(),
            force_flush: false,
        }]);
        backend.run_flush_log().await.unwrap();
        assert!(backend.inner.log_queue.lock().unwrap().is_empty());
        assert_eq!(
            backend
                .inner
                .flush_waiter
                .next_wait_lsn
                .load(Ordering::Acquire),
            2
        );

        let layout = backend.inner.state.lock().unwrap().layout.clone();
        let chunk_path = layout.chunk_path(0);
        let bytes = mudu_sys::fs::sync::read(chunk_path).unwrap();
        assert!(!bytes.is_empty());
    })
    .unwrap();
}
