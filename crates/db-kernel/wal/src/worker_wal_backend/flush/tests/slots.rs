#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::worker_wal_backend::backend::FlushTask;
use futures::task::noop_waker_ref;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};

/// Polls a boxed slot task to completion, giving the tokio blocking
/// writes time to land between polls.
async fn poll_slot_task_to_completion(task: &mut FlushTask) {
    let waker = noop_waker_ref();
    let mut cx = Context::from_waker(waker);
    for _ in 0..1000 {
        let Some(inner) = task.as_mut() else {
            return;
        };
        match inner.as_mut().poll(&mut cx) {
            Poll::Ready(result) => {
                result.unwrap();
                *task = None;
                return;
            }
            Poll::Pending => {}
        }
        mudu_sys::task::async_::sleep(Duration::from_millis(1))
            .await
            .unwrap();
    }
    panic!("slot task did not complete");
}

#[test]
fn second_flush_round_completes_via_free_slot_while_first_slot_stuck() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let backend = make_backend_with_queue(vec![force_queued_batch(0)]);
        // Occupy slot 0 with a never-ready future: rounds must still
        // start and complete through the remaining slots.
        *backend.flush_tasks[0].lock().unwrap() = Some(Box::pin(async move {
            futures::future::pending::<RS<()>>().await
        }));
        let deadline = instant_now() + Duration::from_secs(10);
        loop {
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
            assert!(
                instant_now() < deadline,
                "flush round must complete via a free slot"
            );
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
        assert!(backend.flush_tasks[0].lock().unwrap().is_some());
    })
    .unwrap();
}

#[test]
fn out_of_order_slot_completions_advance_only_contiguous_prefix() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        // One batch per round: each drain takes a single batch, so the
        // two rounds carry LSN 0 and LSN 1 separately.
        let batching = WorkerLogBatching::new(64 * 1024, 32, Duration::from_millis(10), 1);
        let backend = make_backend_with_queue_batching_and_sync_policy(
            vec![force_queued_batch(0)],
            batching,
            WalSyncPolicy::Commit,
        );
        // Start round A (LSN 0) in slot 0; its first poll submits the
        // write and pends. Hold its task so it cannot complete.
        backend.poll_flush_log().unwrap();
        let mut round_a = backend.flush_tasks[0].lock().unwrap().take();
        assert!(round_a.is_some(), "round A must have started in slot 0");

        // Start round B (LSN 1) via another slot and drive it to
        // completion: reporting LSN 1 alone must NOT advance the
        // watermark past the gap at LSN 0.
        backend
            .inner
            .log_queue
            .lock()
            .unwrap()
            .push(force_queued_batch(1));
        backend.poll_flush_log().unwrap();
        let mut round_b = backend.flush_tasks[0].lock().unwrap().take();
        assert!(round_b.is_some(), "round B must have started in slot 0");
        poll_slot_task_to_completion(&mut round_b).await;
        assert_eq!(
            backend
                .inner
                .flush_waiter
                .next_wait_lsn
                .load(Ordering::Acquire),
            0,
            "watermark must wait for the contiguous prefix (LSN 0)"
        );

        // Completing round A reports LSN 0; the prefix is contiguous
        // and the watermark advances past both.
        poll_slot_task_to_completion(&mut round_a).await;
        assert_eq!(
            backend
                .inner
                .flush_waiter
                .next_wait_lsn
                .load(Ordering::Acquire),
            2
        );
    })
    .unwrap();
}

#[test]
fn is_flush_idle_tracks_all_slots() {
    let backend = make_backend_with_queue(vec![]);
    assert!(backend.is_flush_idle().unwrap());
    *backend.flush_tasks[2].lock().unwrap() = Some(Box::pin(async move {
        futures::future::pending::<RS<()>>().await
    }));
    assert!(!backend.is_flush_idle().unwrap());
    backend.flush_tasks[2].lock().unwrap().take();
    assert!(backend.is_flush_idle().unwrap());
    backend
        .inner
        .log_queue
        .lock()
        .unwrap()
        .push(force_queued_batch(0));
    assert!(!backend.is_flush_idle().unwrap());
}
