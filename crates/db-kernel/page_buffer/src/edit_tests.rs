//! Exercise publication ownership and consumer policies independently of MBP1.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
static A: ImageValidator = ImageValidator::new();
static B: ImageValidator = ImageValidator::new();

fn edit(value: u8) -> PageEdit {
    let mut edit = PageEdit::new();
    edit.write(0, &[value]);
    edit
}
#[tokio::test]
async fn unique_edit_reuses_bytes_and_shared_edit_preserves_snapshot() {
    let pool = super::tests::pool();
    let access = pool.access().await;
    let page = super::tests::initialized(&pool, &access, 7).await;
    let pin = pool.pin(&access, &page).await.unwrap();
    let original = pin.snapshot().await;
    original.mark_validated(&A);
    assert!(!original.validated_by(&B));
    let pointer = original.as_ptr();
    drop(original);
    let mut write = pin
        .write()
        .await
        .prepare_edit(edit(8), Some(&A), None)
        .unwrap();
    assert!(!write.copied());
    write.publish();
    write.publish(); // Publication is idempotent.
    drop(write);
    let snapshot = pin.snapshot().await;
    assert_eq!(pointer, snapshot.as_ptr());
    assert_eq!(snapshot[0], 8);
    assert!(snapshot.validated_by(&A));
    let weak = snapshot.identity();
    let mut write = pin.write().await.prepare_edit(edit(9), None, None).unwrap();
    assert!(write.copied());
    write.publish();
    drop(write);
    let latest = pin.snapshot().await;
    assert_eq!(snapshot[0], 8);
    assert_eq!(latest[0], 9);
    assert!(!latest.validated_by(&A));
    assert!(!weak.matches(&latest));
    assert!(weak.matches(&snapshot));
}
#[tokio::test]
async fn invalid_and_cancelled_edits_never_publish() {
    let pool = super::tests::pool();
    let access = pool.access().await;
    let page = super::tests::initialized(&pool, &access, 7).await;
    let pin = pool.pin(&access, &page).await.unwrap();
    let before = pin.snapshot().await;
    let mut invalid = edit(8);
    invalid.copy_within(0..8, usize::MAX);
    assert!(pin.write().await.prepare_edit(invalid, None, None).is_err());
    drop(pin.write().await.prepare_edit(edit(9), None, None).unwrap());
    assert_eq!(&*pin.snapshot().await, &*before);
    pool.flush(&access).await.unwrap();
    assert_eq!(pool.stats().writes, 1);
    drop(before);
    let weak = pin.snapshot().await.identity();
    let mut write = pin
        .write()
        .await
        .prepare_edit(edit(10), None, None)
        .unwrap();
    assert!(!write.copied());
    write.publish();
    drop(write);
    assert!(!weak.matches(&pin.snapshot().await));
}

#[derive(Debug)]
struct Seal {
    fail: AtomicBool,
}
static SEAL: Seal = Seal {
    fail: AtomicBool::new(false),
};
impl PageWriteback for Seal {
    fn encode(&self, bytes: &[u8]) -> RS<Vec<u8>> {
        if self.fail.load(Ordering::Acquire) {
            return Err(invalid("injected writeback failure"));
        }
        let mut bytes = bytes.to_vec();
        bytes[63] = bytes[0];
        Ok(bytes)
    }
}
#[tokio::test]
async fn writeback_policy_seals_a_copy_and_failure_keeps_page_dirty() {
    let store = Arc::new(MemoryPageStore::default());
    let pool = ClockBufferManager::new(
        BufferOptions {
            page_size: 64,
            frames: 8,
            partitions: 1,
            ..BufferOptions::default()
        },
        store.clone(),
        AllocatorState { next_page: 1 },
        None,
    )
    .unwrap();
    let access = pool.access().await;
    let page = super::tests::initialized(&pool, &access, 7).await;
    let pin = pool.pin(&access, &page).await.unwrap();
    let mut write = pin
        .write()
        .await
        .prepare_edit(edit(42), None, Some(&SEAL))
        .unwrap();
    write.publish();
    drop(write);
    SEAL.fail.store(true, Ordering::Release);
    assert!(pool.flush(&access).await.is_err());
    assert_eq!(store.read(pin.id()).await.unwrap()[0], 7);
    SEAL.fail.store(false, Ordering::Release);
    pool.flush(&access).await.unwrap();
    assert_eq!(store.read(pin.id()).await.unwrap()[63], 42);
    assert_eq!(pin.snapshot().await[63], 7);
    assert_eq!(pool.stats().writes, 2);
}

#[tokio::test]
async fn fast_shared_lease_does_not_bypass_queued_maintenance() {
    use futures::{poll, FutureExt};
    let pool = super::tests::pool();
    let reader = pool.try_access().unwrap();
    let mut maintenance = pool.maintenance().boxed_local();
    assert!(poll!(&mut maintenance).is_pending());
    assert!(pool.try_access().is_none());
    drop(reader);
    let exclusive = maintenance.await;
    assert!(pool.try_access().is_none());
    drop(exclusive);
    assert!(pool.try_access().is_some());
}
