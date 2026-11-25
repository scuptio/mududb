//! Local scheduling, cancellation and ownership invariants replacing the shared-frame model.
#![allow(clippy::unwrap_used, clippy::panic)]
use crate::LocalRwLock;
use futures::poll;
use std::rc::Rc;

#[tokio::test(flavor = "current_thread")]
async fn queued_writer_is_fair_and_cancelled_waiter_releases_its_place() {
    let lock = LocalRwLock::new(7);
    let reader = lock.read().await;
    let mut writer = Box::pin(lock.write());
    assert!(poll!(&mut writer).is_pending());
    assert!(lock.try_read().is_none());
    let mut later = Box::pin(lock.read());
    assert!(poll!(&mut later).is_pending());
    drop(writer);
    // A queued shared reader can proceed with an existing reader after cancellation.
    assert_eq!(*later.await, 7);
    drop(reader);
    *lock.write().await = 9;
    assert_eq!(*lock.read().await, 9);
}

#[tokio::test(flavor = "current_thread")]
async fn suspended_writer_wakes_local_readers_after_publication() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let lock = Rc::new(LocalRwLock::new(0));
            let mut writer = lock.write().await;
            let copy = lock.clone();
            let reader = tokio::task::spawn_local(async move { *copy.read().await });
            tokio::task::yield_now().await;
            *writer = 42;
            drop(writer);
            assert_eq!(reader.await.unwrap(), 42);
        })
        .await;
}
