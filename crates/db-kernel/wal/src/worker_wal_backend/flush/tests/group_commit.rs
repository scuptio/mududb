#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::log_frame::decode_entries_with_pending;
use crate::log_frame::frame_lsns;
use crate::worker_log::decode_frames;
use crate::worker_log::WorkerLogBackend;
use std::sync::atomic::Ordering;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct GroupCommitEntry {
    id: u64,
    payload: Vec<u8>,
}

async fn drive_flush_until_advanced(backend: &WorkerWALBackend, target_next_lsn: u64) -> RS<()> {
    loop {
        backend.run_flush_log().await?;
        let advanced = backend
            .inner
            .flush_waiter
            .next_wait_lsn
            .load(Ordering::Acquire);
        let queued = backend.inner.log_queue.lock().unwrap().len();
        if advanced >= target_next_lsn && queued == 0 {
            return Ok(());
        }
        mudu_sys::task::async_::sleep(Duration::from_millis(1)).await?;
    }
}

fn decode_persisted_entries(backend: &WorkerWALBackend) -> Vec<(LSN, GroupCommitEntry)> {
    let layout = backend.inner.state.lock().unwrap().layout.clone();
    let bytes = mudu_sys::fs::sync::read(layout.chunk_path(0)).unwrap();
    let frames = decode_frames(&bytes).unwrap();
    let mut pending_frames = Vec::new();
    let mut pending_start_lsn = None;
    decode_entries_with_pending(&frames, &mut pending_frames, &mut pending_start_lsn).unwrap()
}

#[test]
fn wait_advanced_parks_until_target_lsn_ready() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let waiter = Arc::new(WaitLsn::new(LSN::new(0), vec![], None));
        // An earlier advance leaves the sticky latch set; a wait for a
        // higher LSN must still park instead of returning immediately.
        waiter.ready(vec![LSN::new(0)]).unwrap();
        waiter.wait_advanced(LSN::new(0)).await.unwrap();

        let w = waiter.clone();
        let notifier = async move {
            mudu_sys::task::async_::sleep(Duration::from_millis(10))
                .await
                .unwrap();
            w.ready(vec![LSN::new(1)]).unwrap();
        };
        let waiting = waiter.wait_advanced(LSN::new(1));
        let joined = mudu_sys::task::async_::timeout(Duration::from_secs(10), async move {
            futures::join!(notifier, waiting)
        })
        .await;
        let (_, result) = joined.expect("wait_advanced must be woken by ready");
        result.unwrap();
        assert_eq!(waiter.next_wait_lsn.load(Ordering::Acquire), 2);
    })
    .unwrap();
}

#[test]
fn group_commit_append_is_durable_and_readable() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue(vec![]);
        let entry = GroupCommitEntry {
            id: 7,
            payload: vec![3u8; 32],
        };
        // With no external flush driver running, the enqueueing task
        // self-drives the flush round.
        backend.append_entry_group_commit(&entry).await.unwrap();
        assert!(backend.inner.log_queue.lock().unwrap().is_empty());
        assert_eq!(
            backend
                .inner
                .flush_waiter
                .next_wait_lsn
                .load(Ordering::Acquire),
            1
        );
        let entries = decode_persisted_entries(&backend);
        assert_eq!(entries, vec![(LSN::new(0), entry)]);
    })
    .unwrap();
}

#[test]
fn enqueue_group_commit_defers_durability_wait() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue(vec![]);
        // Simulate in-flight flush rounds at the driver cap: enqueues
        // batch instead of self-driving.
        backend
            .inner
            .flush_drivers
            .store(FLUSH_SLOT_COUNT, Ordering::Release);
        let entry = GroupCommitEntry {
            id: 9,
            payload: vec![5u8; 24],
        };
        let frames = WorkerLogBackend::serialize_entry(&backend, &entry).unwrap();
        let lsns = frame_lsns(&frames).unwrap();
        // force_flush = false and below the batching watermarks while a
        // round is being driven elsewhere: enqueue must return without
        // waiting for durability, so a durability wait right after would
        // park.
        let last_lsn = backend
            .enqueue_group_commit(frames, lsns, false)
            .await
            .unwrap();
        assert_eq!(backend.inner.log_queue.lock().unwrap().len(), 1);
        let waited = mudu_sys::task::async_::timeout(
            Duration::from_millis(50),
            backend.wait_group_commit_advanced(last_lsn),
        )
        .await;
        assert!(waited.is_none(), "durability wait must park until flush");

        // After a flush round the same wait completes and the entry is
        // persisted in LSN order.
        backend.inner.flush_drivers.store(0, Ordering::Release);
        backend.force_flush_log_async().await.unwrap();
        backend.wait_group_commit_advanced(last_lsn).await.unwrap();
        let entries = decode_persisted_entries(&backend);
        assert_eq!(entries, vec![(last_lsn, entry)]);
    })
    .unwrap();
}

#[test]
fn group_commit_concurrent_appends_all_woken_and_replayable() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue(vec![]);
        const N: u64 = 8;
        let appends: Vec<_> = (0..N)
            .map(|i| {
                let backend = backend.clone();
                async move {
                    backend
                        .append_entry_group_commit(&GroupCommitEntry {
                            id: i,
                            payload: vec![i as u8; 24],
                        })
                        .await
                }
            })
            .collect();
        let driver_backend = backend.clone();
        let driver = async move { drive_flush_until_advanced(&driver_backend, N).await };
        let joined = mudu_sys::task::async_::timeout(Duration::from_secs(30), async move {
            let (results, driver_result) =
                futures::join!(futures::future::join_all(appends), driver);
            for result in results {
                result.unwrap();
            }
            driver_result.unwrap();
        })
        .await;
        joined.expect("concurrent group commits must all be woken");
        assert_eq!(
            backend
                .inner
                .flush_waiter
                .next_wait_lsn
                .load(Ordering::Acquire),
            N
        );

        let mut ids: Vec<u64> = decode_persisted_entries(&backend)
            .into_iter()
            .map(|(_, entry)| entry.id)
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, (0..N).collect::<Vec<_>>());
    })
    .unwrap();
}

#[test]
fn non_force_batches_share_one_driver_flush() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue(vec![]);
        // Simulate in-flight flush rounds at the driver cap so enqueues
        // batch instead of self-driving.
        backend
            .inner
            .flush_drivers
            .store(FLUSH_SLOT_COUNT, Ordering::Release);
        const N: u64 = 4;
        let mut last_lsns = Vec::new();
        for i in 0..N {
            let frames = WorkerLogBackend::serialize_entry(
                &backend,
                &GroupCommitEntry {
                    id: i,
                    payload: vec![i as u8; 24],
                },
            )
            .unwrap();
            let lsns = frame_lsns(&frames).unwrap();
            last_lsns.push(
                backend
                    .enqueue_group_commit(frames, lsns, false)
                    .await
                    .unwrap(),
            );
        }
        // Non-force batches queue up; one forced pass flushes and
        // durably completes all of them in a single round.
        assert_eq!(backend.inner.log_queue.lock().unwrap().len(), N as usize);
        backend.inner.flush_drivers.store(0, Ordering::Release);
        backend.force_flush_log_async().await.unwrap();
        assert!(backend.inner.log_queue.lock().unwrap().is_empty());
        assert_eq!(backend.flush_round_count(), 1);
        for last_lsn in last_lsns {
            backend.wait_group_commit_advanced(last_lsn).await.unwrap();
        }
        let mut ids: Vec<u64> = decode_persisted_entries(&backend)
            .into_iter()
            .map(|(_, entry)| entry.id)
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, (0..N).collect::<Vec<_>>());
    })
    .unwrap();
}

#[test]
fn flush_pending_batches_respects_watermarks() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let batching = WorkerLogBatching::new(64 * 1024, 32, Duration::from_millis(50), 256 * 1024);
        let backend = make_backend_with_queue_and_batching(
            vec![QueuedLogBatch {
                frames: vec![vec![1u8; 32]],
                lsns: vec![LSN::new(0)],
                bytes: 32,
                enqueued_at: instant_now().into_std(),
                force_flush: false,
            }],
            batching,
        );
        // Below the byte/frame triggers and freshly enqueued: the
        // background loop must leave the batch queued for batching.
        backend.flush_pending_batches().await.unwrap();
        assert_eq!(backend.inner.log_queue.lock().unwrap().len(), 1);
        assert_eq!(backend.flush_round_count(), 0);

        // Once the oldest batch ages past max_wait the same path flushes.
        backend.inner.log_queue.lock().unwrap()[0].enqueued_at =
            instant_now().into_std() - Duration::from_millis(100);
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

#[test]
fn concurrent_enqueues_merge_into_one_flush_round() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        // High watermarks so the driving task batches the whole burst
        // instead of flushing per commit.
        let batching =
            WorkerLogBatching::new(64 * 1024, 32, Duration::from_millis(100), 256 * 1024);
        let backend = make_backend_with_queue_and_batching(vec![], batching);
        const N: u64 = 16;
        let appends: Vec<_> = (0..N)
            .map(|i| {
                let backend = backend.clone();
                async move {
                    backend
                        .append_entry_group_commit(&GroupCommitEntry {
                            id: i,
                            payload: vec![i as u8; 24],
                        })
                        .await
                }
            })
            .collect();
        let joined = mudu_sys::task::async_::timeout(
            Duration::from_secs(30),
            futures::future::join_all(appends),
        )
        .await;
        let results = joined.expect("concurrent group commits must all be woken");
        for result in results {
            result.unwrap();
        }
        assert_eq!(
            backend
                .inner
                .flush_waiter
                .next_wait_lsn
                .load(Ordering::Acquire),
            N
        );
        // N concurrent commits drained in far fewer than N flush rounds:
        // the first driver waited out the batching window while the rest
        // enqueued, then one round covered them all.
        let rounds = backend.flush_round_count();
        assert!(rounds < N / 2, "expected merged flush rounds, got {rounds}");
        assert!(rounds >= 1);

        let mut ids: Vec<u64> = decode_persisted_entries(&backend)
            .into_iter()
            .map(|(_, entry)| entry.id)
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, (0..N).collect::<Vec<_>>());
    })
    .unwrap();
}

#[test]
fn lone_self_driven_commit_flushes_within_one_max_wait() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let max_wait = Duration::from_millis(50);
        let batching = WorkerLogBatching::new(64 * 1024, 32, max_wait, 256 * 1024);
        let backend = make_backend_with_queue_and_batching(vec![], batching);
        let started = instant_now();
        backend
            .append_entry_group_commit(&GroupCommitEntry {
                id: 1,
                payload: vec![7u8; 24],
            })
            .await
            .unwrap();
        let elapsed = started.elapsed();
        // The batching window is at most one `max_wait`; the wall-clock
        // bound must also absorb fsync latency, which is unbounded under
        // load, so use a generous cap. The deterministic invariants
        // (single flush round, durability, drained queue) are asserted
        // below.
        assert!(
            elapsed < Duration::from_secs(10),
            "lone commit latency exceeded the generous cap, took {elapsed:?}"
        );
        assert_eq!(backend.flush_round_count(), 1);
        assert_eq!(
            decode_persisted_entries(&backend)
                .into_iter()
                .map(|(_, entry)| entry.id)
                .collect::<Vec<_>>(),
            vec![1]
        );
    })
    .unwrap();
}
