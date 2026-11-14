#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use futures::{poll, FutureExt};
use mudu_sys::sync::async_::AMutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Default)]
struct TestFile {
    bytes: AMutex<Vec<u8>>,
    hold_clean: AtomicBool,
    reached_clean: AtomicBool,
    step: AtomicUsize,
    fail_at: AtomicUsize,
}
impl TestFile {
    fn fault(&self) -> RS<()> {
        let step = self.step.fetch_add(1, Ordering::AcqRel) + 1;
        if self.fail_at.load(Ordering::Acquire) == step {
            return Err(mudu_error!(ErrorCode::Storage, "injected manifest failure"));
        }
        Ok(())
    }
}
#[async_trait::async_trait]
impl AsyncFile for TestFile {
    async fn read_exact_at(&self, at: u64, len: usize) -> RS<Vec<u8>> {
        let data = self.bytes.lock().await;
        data.get(at as usize..at as usize + len)
            .map(|s| s.to_vec())
            .ok_or_else(|| mudu_error!(ErrorCode::Storage, "short test read"))
    }
    async fn write_all_at(&self, at: u64, data: &[u8]) -> RS<()> {
        if at == 0 && data == [1] {
            self.reached_clean.store(true, Ordering::Release);
            std::future::poll_fn(|cx| {
                if self.hold_clean.load(Ordering::Acquire) {
                    cx.waker().wake_by_ref();
                    std::task::Poll::Pending
                } else {
                    std::task::Poll::Ready(())
                }
            })
            .await;
        }
        let mut bytes = self.bytes.lock().await;
        let end = at as usize + data.len();
        if bytes.len() < end {
            bytes.resize(end, 0);
        }
        bytes[at as usize..end].copy_from_slice(data);
        self.fault()
    }
    async fn fsync(&self) -> RS<()> {
        self.fault()
    }
    async fn file_len(&self) -> RS<u64> {
        Ok(self.bytes.lock().await.len() as u64)
    }
}
async fn paged(file: Arc<TestFile>) -> PagedRelation {
    let schema = super::super::tests::test_schema();
    let desc = catalog::contract::table_info::TableInfo::new(schema)
        .unwrap()
        .table_desc()
        .unwrap();
    let context = CompareContext {
        result: Ok(()),
        comparator: mudu_contract::tuple::comparator::TupleComparator::new(),
        desc: desc.key_desc().clone(),
    };
    PagedRelation {
        tree: PagedBTree::with_row_ids(
            context,
            BufferOptions::default(),
            Arc::new(::index::paged::MemoryPageStore::default()),
        )
        .unwrap(),
        rows: rows::Rows::new(Arc::new(TestFile::default()), Arc::new(TestFile::default()))
            .await
            .unwrap(),
        manifest: Arc::new(OrderedFile::new(file)),
        dirty: AMutex::new(true),
    }
}

#[tokio::test]
async fn cancelled_clean_publication_is_drained_before_write_invalidation() {
    let file = Arc::new(TestFile::default());
    file.hold_clean.store(true, Ordering::Release);
    let paged = paged(file.clone()).await;
    let mut checkpoint = paged.checkpoint(vec![1], 1, false).boxed_local();
    for _ in 0..1000 {
        assert!(poll!(&mut checkpoint).is_pending());
        if file.reached_clean.load(Ordering::Acquire) {
            break;
        }
        mudu_utils::yield_now::cooperative_yield_now().await;
    }
    assert!(file.reached_clean.load(Ordering::Acquire));
    drop(checkpoint);
    assert!(!*paged.dirty.lock().await);
    file.hold_clean.store(false, Ordering::Release);
    paged.before_write().await.unwrap();
    assert_eq!(file.read_exact_at(0, 1).await.unwrap(), vec![0]);
    paged.checkpoint(vec![2], 2, false).await.unwrap();
    assert_eq!(file.read_exact_at(0, 1).await.unwrap(), vec![1]);
    assert_eq!(
        manifest::consume(paged.manifest.as_ref())
            .await
            .unwrap()
            .unwrap()
            .stamp,
        vec![2]
    );
}

#[tokio::test]
async fn every_publication_failure_requires_invalidation_before_retry() {
    for fail_at in 1..=7 {
        let file = Arc::new(TestFile::default());
        file.fail_at.store(fail_at, Ordering::Release);
        let paged = paged(file.clone()).await;
        assert!(paged.checkpoint(vec![1], 1, false).await.is_err());
        assert!(!*paged.dirty.lock().await);
        file.fail_at.store(0, Ordering::Release);
        paged.before_write().await.unwrap();
        assert_eq!(file.read_exact_at(0, 1).await.unwrap(), vec![0]);
        paged.checkpoint(vec![2], 2, false).await.unwrap();
        assert_eq!(
            manifest::consume(paged.manifest.as_ref())
                .await
                .unwrap()
                .unwrap()
                .stamp,
            vec![2]
        );
    }
}

#[tokio::test]
async fn failed_invalidation_is_retried_and_does_not_admit_writes() {
    let file = Arc::new(TestFile::default());
    let paged = paged(file.clone()).await;
    paged.checkpoint(vec![1], 1, false).await.unwrap();
    file.fail_at
        .store(file.step.load(Ordering::Acquire) + 2, Ordering::Release);
    assert!(paged.before_write().await.is_err());
    assert!(!*paged.dirty.lock().await);
    file.fail_at.store(0, Ordering::Release);
    paged.before_write().await.unwrap();
    assert!(*paged.dirty.lock().await);
    assert_eq!(file.read_exact_at(0, 1).await.unwrap(), vec![0]);
}

/// Only the mailbox moves to another thread; requests execute on the creator's LocalSet.
#[tokio::test(flavor = "current_thread")]
async fn paged_mailbox_routes_cross_thread_requests_without_sharing_tree() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let handle = Handle::new(paged(Arc::new(TestFile::default())).await).unwrap();
            let (done, result) = tokio::sync::oneshot::channel();
            mudu_sys::task::sync::spawn_thread(move || {
                let output = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
                    handle
                        .call(|relation| Box::pin(async move { relation.tree.len().await }))
                        .await
                });
                let _ = done.send(output);
            })
            .unwrap();
            assert_eq!(result.await.unwrap().unwrap().unwrap(), 0);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn cancelled_mailbox_reply_cannot_publish_a_late_tree_edit() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let handle = Handle::new(paged(Arc::new(TestFile::default())).await).unwrap();
            handle
                .call(|relation| {
                    Box::pin(async move {
                        relation
                            .tree
                            .insert(KeyTuple::from(super::super::tests::i32_bytes(1)), 1)
                            .await
                    })
                })
                .await
                .unwrap();
            for start_driver in [false, true] {
                let (resume, blocked) = tokio::sync::oneshot::channel();
                let reply = handle.call(move |relation| {
                    Box::pin(async move {
                        let _ = blocked.await;
                        relation.tree.clear().await
                    })
                });
                if start_driver {
                    tokio::task::yield_now().await;
                }
                drop(reply);
                let _ = resume.send(());
                tokio::task::yield_now().await;
                assert_eq!(
                    handle
                        .call(|relation| Box::pin(async move { relation.tree.len().await }))
                        .await
                        .unwrap(),
                    1
                );
            }
        })
        .await;
}
