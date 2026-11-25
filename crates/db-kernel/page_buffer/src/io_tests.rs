#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use async_trait::async_trait;
use futures::{poll, FutureExt};
use mudu_sys::{contract::async_file::AsyncFile, sync::async_::AMutex};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Default)]
struct DelayedFile {
    hold: AtomicBool,
    bytes: AMutex<Vec<u8>>,
}
#[async_trait]
impl AsyncFile for DelayedFile {
    async fn write_all_at(&self, offset: u64, bytes: &[u8]) -> RS<()> {
        // Simulates a submitted write completing after caller cancellation.
        std::future::poll_fn(|cx| {
            if self.hold.load(Ordering::Acquire) {
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            } else {
                std::task::Poll::Ready(())
            }
        })
        .await;
        let mut out = self.bytes.lock().await;
        let end = offset as usize + bytes.len();
        if out.len() < end {
            out.resize(end, 0);
        }
        out[offset as usize..end].copy_from_slice(bytes);
        Ok(())
    }
    async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        let bytes = self.bytes.lock().await;
        bytes
            .get(offset as usize..offset as usize + len)
            .map(|b| b.to_vec())
            .ok_or_else(|| invalid("short read"))
    }
    async fn file_len(&self) -> RS<u64> {
        Ok(self.bytes.lock().await.len() as u64)
    }
    async fn fsync(&self) -> RS<()> {
        Ok(())
    }
}

#[tokio::test]
async fn cancelled_write_is_drained_before_retry() {
    let file = Arc::new(DelayedFile::default());
    file.hold.store(true, Ordering::Release);
    let ordered = OrderedFile::new(file.clone());
    let mut first = ordered.write_all_at(0, b"old").boxed_local();
    assert!(poll!(&mut first).is_pending());
    drop(first);
    file.hold.store(false, Ordering::Release);
    ordered.write_all_at(0, b"new").await.unwrap();
    assert_eq!(ordered.read_exact_at(0, 3).await.unwrap(), b"new");
}

#[derive(Default)]
struct FaultStore {
    write_gate: AtomicBool,
    write_entered: AtomicBool,
    inner: MemoryPageStore,
    fail: AtomicBool,
    hold: AtomicBool,
}
#[async_trait]
impl PageStore for FaultStore {
    async fn read(&self, id: u64) -> RS<Vec<u8>> {
        if self.hold.load(Ordering::Acquire) {
            std::future::pending::<()>().await;
        }
        self.inner.read(id).await
    }
    async fn write(&self, id: u64, bytes: &[u8]) -> RS<()> {
        if self.write_gate.load(Ordering::Acquire) {
            self.write_entered.store(true, Ordering::Release);
            std::future::poll_fn(|cx| {
                if self.write_gate.load(Ordering::Acquire) {
                    cx.waker().wake_by_ref();
                    std::task::Poll::Pending
                } else {
                    std::task::Poll::Ready(())
                }
            })
            .await;
        }
        if self.fail.load(Ordering::Acquire) {
            return Err(invalid("injected write failure"));
        }
        self.inner.write(id, bytes).await
    }
    async fn flush(&self) -> RS<()> {
        Ok(())
    }
}
#[tokio::test]
async fn file_page_store_bias_maps_logical_pages_onto_backing_offsets() {
    let file = Arc::new(DelayedFile::default());
    let store = FilePageStore::with_page_bias(file.clone(), 64, 1).unwrap();
    store.write(1, &[7; 64]).await.unwrap();
    store.write(2, &[8; 64]).await.unwrap();
    assert_eq!(store.read(1).await.unwrap(), vec![7; 64]);
    assert_eq!(store.read(2).await.unwrap(), vec![8; 64]);
    // A logical page below the bias never touches the backing file.
    assert!(store.read(0).await.is_err());
    assert_eq!(file.read_exact_at(0, 64).await.unwrap(), vec![7; 64]);
    assert_eq!(file.read_exact_at(64, 64).await.unwrap(), vec![8; 64]);
}

#[tokio::test]
async fn cancelled_miss_releases_claim_and_write_failure_keeps_dirty_page() {
    let store = Arc::new(FaultStore {
        inner: MemoryPageStore::default(),
        fail: AtomicBool::new(false),
        hold: AtomicBool::new(false),
        ..FaultStore::default()
    });
    let pool = ClockBufferManager::new(
        BufferOptions {
            frames: 8,
            partitions: 1,
            page_size: 512,
            ..BufferOptions::default()
        },
        store.clone(),
        AllocatorState { next_page: 2 },
        Some((1, PageImage::new(vec![0; 512]))),
    )
    .unwrap();
    let access = pool.access().await;
    store.fail.store(true, Ordering::Release);
    assert!(pool.flush(&access).await.is_err());
    store.fail.store(false, Ordering::Release);
    pool.flush(&access).await.unwrap();
    let root = pool.page_ref(1);
    drop(pool.pin(&access, &root).await.unwrap());
    for i in 0..30 {
        let reserved = pool.reserve_page(&access).await.unwrap();
        let link = pool
            .initialize(&access, reserved, PageImage::new(vec![i; 512]))
            .await
            .unwrap();
        drop(pool.pin(&access, &link).await.unwrap());
    }
    store.hold.store(true, Ordering::Release);
    let mut miss = pool.pin(&access, &root).boxed_local();
    assert!(poll!(&mut miss).is_pending());
    drop(miss);
    store.hold.store(false, Ordering::Release);
    let pin = pool.pin(&access, &root).await.unwrap();
    assert_eq!(&*pin.snapshot().await, &vec![0; 512]);
}

#[tokio::test]
async fn concurrent_replacement_remains_dirty_after_an_older_flush() {
    let store = Arc::new(FaultStore::default());
    let pool = ClockBufferManager::new(
        BufferOptions {
            page_size: 64,
            frames: 8,
            partitions: 1,
            ..BufferOptions::default()
        },
        store.clone(),
        AllocatorState { next_page: 2 },
        Some((1, PageImage::new(vec![1; 64]))),
    )
    .unwrap();
    let access = pool.access().await;
    let pin = pool.pin(&access, &pool.page_ref(1)).await.unwrap();
    store.write_gate.store(true, Ordering::Release);
    let mut flush = pool.flush(&access).boxed_local();
    for _ in 0..1000 {
        assert!(poll!(&mut flush).is_pending());
        if store.write_entered.load(Ordering::Acquire) {
            break;
        }
        let mut yielded = false;
        std::future::poll_fn(|cx| {
            if yielded {
                std::task::Poll::Ready(())
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            }
        })
        .await;
    }
    assert!(store.write_entered.load(Ordering::Acquire));
    let mut edit = PageEdit::new();
    edit.fill(0..64, 2);
    let mut pending = pin.write().await.prepare_edit(edit, None, None).unwrap();
    assert!(
        pending.copied(),
        "in-flight writeback retains the older snapshot"
    );
    pending.publish();
    drop(pending);
    store.write_gate.store(false, Ordering::Release);
    flush.await.unwrap();
    assert_eq!(store.read(1).await.unwrap(), vec![1; 64]);
    pool.flush(&access).await.unwrap();
    assert_eq!(store.read(1).await.unwrap(), vec![2; 64]);
}

#[tokio::test]
async fn cancelled_initialization_leaves_an_orphan_that_can_be_reused() {
    let store = Arc::new(FaultStore::default());
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
    let id = reservation.id();
    store.write_gate.store(true, Ordering::Release);
    let mut initialize = pool
        .initialize(&access, reservation, PageImage::new(vec![1; 64]))
        .boxed_local();
    assert!(poll!(&mut initialize).is_pending());
    drop(initialize);
    assert!(pool.pin(&access, &pool.page_ref(id)).await.is_err());
    drop(access);
    let exclusive = pool.maintenance().await;
    assert_eq!(pool.reclaim(&exclusive, &HashSet::new()).await.unwrap(), 1);
    store.write_gate.store(false, Ordering::Release);
    let reservation = pool.reserve_page(&exclusive).await.unwrap();
    assert_eq!(reservation.id(), id);
    let page = pool
        .initialize(&exclusive, reservation, PageImage::new(vec![2; 64]))
        .await
        .unwrap();
    assert_eq!(
        &*pool.pin(&exclusive, &page).await.unwrap().snapshot().await,
        &vec![2; 64]
    );
}
