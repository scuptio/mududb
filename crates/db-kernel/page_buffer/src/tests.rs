#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use futures::{poll, FutureExt};
use std::sync::Arc;

pub(super) fn pool() -> ClockBufferManager {
    ClockBufferManager::new(
        BufferOptions {
            page_size: 64,
            frames: 8,
            partitions: 1,
            ..BufferOptions::default()
        },
        Arc::new(MemoryPageStore::default()),
        AllocatorState { next_page: 1 },
        None,
    )
    .unwrap()
}
pub(super) async fn initialized(
    pool: &dyn BufferManager,
    access: &BufferAccess<'_>,
    byte: u8,
) -> PageRef {
    let reserved = pool.reserve_page(access).await.unwrap();
    pool.initialize(
        access,
        reserved,
        PageImage::new(vec![byte; pool.page_size()]),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn arbitrary_byte_pages_and_prepared_writes_have_no_tree_dependency() {
    let pool: std::rc::Rc<dyn BufferManager> = std::rc::Rc::new(pool());
    let access = pool.access().await;
    let page = initialized(pool.as_ref(), &access, 0x99).await;
    let pin = pool.pin(&access, &page).await.unwrap();
    assert_eq!(&*pin.snapshot().await, &vec![0x99; 64]);
    assert!(pin
        .write()
        .await
        .prepare(PageImage::new(vec![1; 63]))
        .is_err());
    let mut pending = pin
        .write()
        .await
        .prepare(PageImage::new(vec![0x55; 64]))
        .unwrap();
    pending.publish();
    assert!(
        pin.try_write().is_none(),
        "publish must retain the page latch"
    );
    drop(pending);
    assert_eq!(&*pin.snapshot().await, &vec![0x55; 64]);
    pool.flush(&access).await.unwrap();
    assert_eq!(pool.stats().writes, 2);
}

#[tokio::test]
async fn wrong_manager_leases_references_and_reservations_are_rejected() {
    let a = pool();
    let b = pool();
    let aa = a.access().await;
    let bb = b.access().await;
    let page = initialized(&a, &aa, 1).await;
    assert!(a.try_pin(&bb, &page).is_err());
    assert!(b.try_pin(&bb, &page).is_err());
    assert!(b.flush(&aa).await.is_err());
    let reservation = a.reserve_page(&aa).await.unwrap();
    assert!(b
        .initialize(&bb, reservation, PageImage::new(vec![2; 64]))
        .await
        .is_err());
    assert!(a.reclaim(&aa, &HashSet::new()).await.is_err());
}

#[tokio::test]
async fn reservations_are_unreadable_and_failed_initialization_is_reclaimable() {
    let pool = pool();
    let access = pool.access().await;
    let reservation = pool.reserve_page(&access).await.unwrap();
    let page = pool.page_ref(reservation.id());
    assert!(pool.pin(&access, &page).await.is_err());
    assert!(pool
        .initialize(&access, reservation, PageImage::new(vec![1; 63]))
        .await
        .is_err());
    drop(access);
    let exclusive = pool.maintenance().await;
    assert_eq!(pool.reclaim(&exclusive, &HashSet::new()).await.unwrap(), 1);
    let reused = initialized(&pool, &exclusive, 2).await;
    assert_eq!(page.id(), reused.id());
    assert_eq!(
        &*pool
            .pin(&exclusive, &reused)
            .await
            .unwrap()
            .snapshot()
            .await,
        &vec![2; 64]
    );
}

#[tokio::test]
async fn maintenance_waits_for_the_whole_traversal_and_old_hints_cannot_observe_reuse() {
    let pool = pool();
    let access = pool.access().await;
    let page = initialized(&pool, &access, 1).await;
    drop(pool.pin(&access, &page).await.unwrap());
    let mut maintenance = pool.maintenance().boxed_local();
    assert!(poll!(&mut maintenance).is_pending());
    // No frame is pinned, but copied references are still protected by the lease.
    drop(access);
    let exclusive = maintenance.await;
    pool.reclaim(&exclusive, &HashSet::new()).await.unwrap();
    let reused = initialized(&pool, &exclusive, 9).await;
    assert_eq!(reused.id(), page.id());
    assert_eq!(
        &*pool
            .pin(&exclusive, &reused)
            .await
            .unwrap()
            .snapshot()
            .await,
        &vec![9; 64]
    );
}

#[tokio::test]
async fn pinned_capacity_exhaustion_returns_error_without_waiting_forever() {
    let pool = pool();
    let access = pool.access().await;
    let mut pins = Vec::new();
    for byte in 0..8 {
        let page = initialized(&pool, &access, byte).await;
        pins.push(pool.pin(&access, &page).await.unwrap());
    }
    let page = initialized(&pool, &access, 9).await;
    assert!(pool.pin(&access, &page).await.is_err());
    pins.pop();
    assert!(pool.pin(&access, &page).await.is_ok());
}

#[tokio::test]
async fn maintenance_never_reuses_an_active_reservation_even_with_the_same_lease() {
    let pool = pool();
    let access = pool.maintenance().await;
    let reservation = pool.reserve_page(&access).await.unwrap();
    assert!(pool.reclaim(&access, &HashSet::new()).await.is_err());
    drop(reservation);
    assert_eq!(pool.reclaim(&access, &HashSet::new()).await.unwrap(), 1);
}

#[tokio::test]
async fn initialize_resident_publishes_dirty_page_without_backing_write() {
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
    let reservation = pool.reserve_page(&access).await.unwrap();
    let page = pool
        .initialize_resident(&access, reservation, PageImage::new(vec![7; 64]))
        .await
        .unwrap();
    // Resident and readable, but nothing reached the backing store yet.
    let pin = pool.pin(&access, &page).await.unwrap();
    assert_eq!(&*pin.snapshot().await, &vec![7; 64]);
    drop(pin);
    assert!(store.read(page.id()).await.is_err());
    assert_eq!(pool.dirty_count(), 1);
    // Writeback persists the image; fsync remains the caller's choice.
    pool.writeback_dirty(&access).await.unwrap();
    assert_eq!(pool.dirty_count(), 0);
    assert_eq!(store.read(page.id()).await.unwrap(), vec![7; 64]);
}

#[tokio::test]
async fn abandoned_reservation_is_recycled_without_maintenance() {
    let pool = pool();
    let access = pool.access().await;
    let abandoned = pool.reserve_page(&access).await.unwrap();
    let id = abandoned.id();
    drop(abandoned);
    // No reclaim: the next reserve sweeps the dead ticket and reuses the id.
    let recycled = pool.reserve_page(&access).await.unwrap();
    assert_eq!(recycled.id(), id);
    let page = pool
        .initialize_resident(&access, recycled, PageImage::new(vec![3; 64]))
        .await
        .unwrap();
    assert_eq!(
        &*pool.pin(&access, &page).await.unwrap().snapshot().await,
        &vec![3; 64]
    );
}

#[tokio::test]
async fn invalid_second_replacement_does_not_publish_the_prepared_first_page() {
    let pool = pool();
    let access = pool.access().await;
    let a = initialized(&pool, &access, 1).await;
    let b = initialized(&pool, &access, 2).await;
    let a = pool.pin(&access, &a).await.unwrap();
    let b = pool.pin(&access, &b).await.unwrap();
    let first = a
        .write()
        .await
        .prepare(PageImage::new(vec![3; 64]))
        .unwrap();
    assert!(b
        .write()
        .await
        .prepare(PageImage::new(vec![4; 63]))
        .is_err());
    drop(first);
    assert_eq!(&*a.snapshot().await, &vec![1; 64]);
    assert_eq!(&*b.snapshot().await, &vec![2; 64]);
}
