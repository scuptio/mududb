#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

#[test]
fn periodic_flush_is_write_only_and_tracks_dirty_paths() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue_batching_and_sync_policy(
            vec![force_queued_batch(0)],
            WorkerLogBatching::default(),
            WalSyncPolicy::Periodic {
                interval: Duration::from_secs(3600),
            },
        );
        backend.run_flush_log().await.unwrap();
        // The waiter watermark advanced on the write alone, without
        // waiting for an fsync...
        assert_eq!(
            backend
                .inner
                .flush_waiter
                .next_wait_lsn
                .load(Ordering::Acquire),
            1
        );
        // ...and the chunk path is tracked for the periodic fsync.
        let layout = backend.inner.state.lock().unwrap().layout.clone();
        {
            let unsynced = backend.inner.unsynced_paths.lock().unwrap();
            assert_eq!(unsynced.len(), 1);
            assert!(unsynced.contains(&layout.chunk_path(0)));
        }
        assert!(backend.has_pending_periodic_fsync().unwrap());
        // The write itself still landed in the file.
        let bytes = mudu_sys::fs::sync::read(layout.chunk_path(0)).unwrap();
        assert_eq!(bytes.len(), 32);
    })
    .unwrap();
}

#[test]
fn periodic_flush_stays_write_only_after_interval() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue_batching_and_sync_policy(
            vec![force_queued_batch(0)],
            WorkerLogBatching::default(),
            WalSyncPolicy::Periodic {
                interval: Duration::from_millis(10),
            },
        );
        backend.run_flush_log().await.unwrap();
        assert_eq!(backend.inner.unsynced_paths.lock().unwrap().len(), 1);

        // Even with last_fsync aged beyond the interval, flush rounds
        // stay write-only: fsync is the fsync task's job, never the
        // write round's.
        let aged = instant_now().into_std() - Duration::from_secs(3600);
        *backend.inner.last_fsync.lock().unwrap() = aged;
        backend
            .inner
            .log_queue
            .lock()
            .unwrap()
            .push(force_queued_batch(1));
        backend.run_flush_log().await.unwrap();
        assert_eq!(backend.inner.unsynced_paths.lock().unwrap().len(), 1);
        assert!(backend.has_pending_periodic_fsync().unwrap());
        assert_eq!(*backend.inner.last_fsync.lock().unwrap(), aged);
        assert_eq!(
            backend
                .inner
                .flush_waiter
                .next_wait_lsn
                .load(Ordering::Acquire),
            2
        );

        // The fsync path clears the dirty set and updates last_fsync.
        backend.fsync_unsynced_paths().await.unwrap();
        assert!(backend.inner.unsynced_paths.lock().unwrap().is_empty());
        assert!(*backend.inner.last_fsync.lock().unwrap() > aged);
    })
    .unwrap();
}

#[test]
fn commit_mode_flush_never_tracks_unsynced_paths() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue(vec![force_queued_batch(0)]);
        backend.run_flush_log().await.unwrap();
        assert!(backend.inner.unsynced_paths.lock().unwrap().is_empty());
        assert!(!backend.has_pending_periodic_fsync().unwrap());
        assert_eq!(
            backend
                .inner
                .flush_waiter
                .next_wait_lsn
                .load(Ordering::Acquire),
            1
        );
    })
    .unwrap();
}

#[test]
fn maybe_periodic_fsync_respects_interval_and_clears_dirty_paths() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue_batching_and_sync_policy(
            vec![force_queued_batch(0)],
            WorkerLogBatching::default(),
            WalSyncPolicy::Periodic {
                interval: Duration::from_millis(10),
            },
        );
        backend.run_flush_log().await.unwrap();
        assert_eq!(backend.inner.unsynced_paths.lock().unwrap().len(), 1);

        // Interval not elapsed: the periodic driver leaves the path dirty.
        backend.maybe_periodic_fsync().await.unwrap();
        assert_eq!(backend.inner.unsynced_paths.lock().unwrap().len(), 1);

        // Interval elapsed: the path is fsynced and untracked.
        let aged = instant_now().into_std() - Duration::from_secs(3600);
        *backend.inner.last_fsync.lock().unwrap() = aged;
        backend.maybe_periodic_fsync().await.unwrap();
        assert!(backend.inner.unsynced_paths.lock().unwrap().is_empty());
        assert!(*backend.inner.last_fsync.lock().unwrap() > aged);
    })
    .unwrap();
}

#[test]
fn maybe_periodic_fsync_is_noop_in_commit_mode() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue(vec![]);
        backend
            .inner
            .unsynced_paths
            .lock()
            .unwrap()
            .insert(PathBuf::from("/tmp/never_fsynced.xl"));
        // Commit mode must not touch the dirty set at all.
        backend.maybe_periodic_fsync().await.unwrap();
        assert_eq!(backend.inner.unsynced_paths.lock().unwrap().len(), 1);
    })
    .unwrap();
}

#[test]
fn fsync_unsynced_paths_fsyncs_regardless_of_interval() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue_batching_and_sync_policy(
            vec![force_queued_batch(0)],
            WorkerLogBatching::default(),
            WalSyncPolicy::Periodic {
                interval: Duration::from_secs(3600),
            },
        );
        backend.run_flush_log().await.unwrap();
        assert_eq!(backend.inner.unsynced_paths.lock().unwrap().len(), 1);
        // The shutdown path forces the fsync even though the interval
        // has not elapsed.
        backend.fsync_unsynced_paths().await.unwrap();
        assert!(backend.inner.unsynced_paths.lock().unwrap().is_empty());
    })
    .unwrap();
}

#[test]
fn next_periodic_fsync_deadline_is_none_in_commit_mode() {
    let backend = make_backend_with_queue(vec![]);
    backend
        .inner
        .unsynced_paths
        .lock()
        .unwrap()
        .insert(PathBuf::from("/tmp/dirty.xl"));
    assert!(backend.next_periodic_fsync_deadline().unwrap().is_none());
}

#[test]
fn next_periodic_fsync_deadline_tracks_last_fsync_plus_interval() {
    let interval = Duration::from_secs(60);
    let backend = make_backend_with_queue_batching_and_sync_policy(
        vec![],
        WorkerLogBatching::default(),
        WalSyncPolicy::Periodic { interval },
    );
    // Nothing dirty: no deadline.
    assert!(backend.next_periodic_fsync_deadline().unwrap().is_none());
    backend
        .inner
        .unsynced_paths
        .lock()
        .unwrap()
        .insert(PathBuf::from("/tmp/dirty.xl"));
    let last_fsync = *backend.inner.last_fsync.lock().unwrap();
    let deadline = backend.next_periodic_fsync_deadline().unwrap().unwrap();
    assert_eq!(deadline, last_fsync + interval);
}

#[test]
fn next_periodic_fsync_deadline_suppressed_by_fsync_slot_not_flush_slot() {
    let interval = Duration::from_secs(60);
    let backend = make_backend_with_queue_batching_and_sync_policy(
        vec![],
        WorkerLogBatching::default(),
        WalSyncPolicy::Periodic { interval },
    );
    backend
        .inner
        .unsynced_paths
        .lock()
        .unwrap()
        .insert(PathBuf::from("/tmp/dirty.xl"));
    // An occupied FLUSH slot no longer suppresses the fsync deadline:
    // write rounds and fsyncs run in independent task slots.
    *backend.flush_tasks[0].lock().unwrap() = Some(Box::pin(async move {
        futures::future::pending::<RS<()>>().await
    }));
    assert!(backend.next_periodic_fsync_deadline().unwrap().is_some());
    // An occupied FSYNC slot suppresses it (the in-flight fsync's
    // completion wakes the event loop).
    *backend.fsync_task.lock().unwrap() = Some(Box::pin(async move {
        futures::future::pending::<RS<()>>().await
    }));
    assert!(backend.next_periodic_fsync_deadline().unwrap().is_none());
}

#[test]
fn poll_flush_log_completes_write_round_while_fsync_slot_occupied() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue_batching_and_sync_policy(
            vec![force_queued_batch(0)],
            WorkerLogBatching::default(),
            WalSyncPolicy::Periodic {
                interval: Duration::from_secs(3600),
            },
        );
        // Occupy the fsync slot with a never-ready future: the flush
        // slot must still start and complete a write round.
        *backend.fsync_task.lock().unwrap() = Some(Box::pin(async move {
            futures::future::pending::<RS<()>>().await
        }));
        for _ in 0..100 {
            backend.poll_flush_log().unwrap();
            if backend
                .inner
                .flush_waiter
                .next_wait_lsn
                .load(Ordering::Acquire)
                == 1
            {
                break;
            }
            mudu_sys::task::async_::sleep(Duration::from_millis(1))
                .await
                .unwrap();
        }
        assert_eq!(
            backend
                .inner
                .flush_waiter
                .next_wait_lsn
                .load(Ordering::Acquire),
            1
        );
        assert!(backend.inner.log_queue.lock().unwrap().is_empty());
        assert_eq!(backend.inner.unsynced_paths.lock().unwrap().len(), 1);
        // The never-ready fsync task is still parked in its own slot and
        // poll_fsync_task drives it without disturbing the flush path.
        assert!(backend.fsync_task.lock().unwrap().is_some());
        assert!(backend.poll_fsync_task().unwrap());
        assert!(backend.fsync_task.lock().unwrap().is_some());
        // Empty fsync slot polls to false.
        backend.fsync_task.lock().unwrap().take();
        assert!(!backend.poll_fsync_task().unwrap());
    })
    .unwrap();
}

#[test]
fn periodic_effective_batching_zeroes_max_wait() {
    let backend = make_backend_with_queue_batching_and_sync_policy(
        vec![],
        WorkerLogBatching::default(),
        WalSyncPolicy::Periodic {
            interval: Duration::from_millis(10),
        },
    );
    let batching = backend.effective_batching();
    assert_eq!(batching.max_wait, Duration::ZERO);
    // Base (unscaled) trigger watermarks are kept.
    assert_eq!(batching.trigger_bytes, 64 * 1024);
    assert_eq!(batching.trigger_frames, 32);
    // The tokio idle interval is floored so the driver cannot busy-spin.
    assert_eq!(backend.flush_idle_interval(), Duration::from_micros(200));
}

#[test]
fn commit_effective_batching_unchanged() {
    let backend = make_backend_with_queue(vec![]);
    let batching = backend.effective_batching();
    assert_eq!(batching.max_wait, Duration::from_micros(200));
    assert_eq!(backend.flush_idle_interval(), Duration::from_micros(200));
    // Adaptive scaling still applies in commit mode: 7 steps of 8
    // sessions each doubles the window past the 5ms cap.
    backend
        .inner
        .active_sessions
        .store(8 * 7, Ordering::Relaxed);
    let scaled = backend.effective_batching();
    assert_eq!(scaled.max_wait, Duration::from_millis(5));
}

#[test]
fn periodic_queued_batch_flushes_immediately() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue_batching_and_sync_policy(
            vec![QueuedLogBatch {
                frames: vec![vec![1u8; 32]],
                lsns: vec![LSN::new(0)],
                bytes: 32,
                enqueued_at: instant_now().into_std(),
                force_flush: false,
            }],
            WorkerLogBatching::default(),
            WalSyncPolicy::Periodic {
                interval: Duration::from_millis(10),
            },
        );
        // A freshly enqueued, non-force batch satisfies the age trigger
        // immediately when max_wait is zero.
        assert!(WorkerWALBackend::should_start_flush(
            backend.inner.log_queue.lock().unwrap().as_slice(),
            backend.effective_batching(),
        ));
        backend.flush_pending_batches().await.unwrap();
        assert!(backend.inner.log_queue.lock().unwrap().is_empty());
        assert_eq!(backend.flush_round_count(), 1);
        assert_eq!(
            backend
                .inner
                .flush_waiter
                .next_wait_lsn
                .load(Ordering::Acquire),
            1
        );
    })
    .unwrap();
}
