#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::tests::{i32_bytes, relation_path, test_schema};
use super::*;
use catalog::contract::{
    snapshot::{RunningXList, Snapshot},
    table_info::TableInfo,
};
use mudu_sys::contract::file_options::FileOptions;

#[tokio::test]
async fn clean_close_restores_versions_and_consumes_marker() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let schema = test_schema();
            let desc = TableInfo::new(schema.clone())
                .unwrap()
                .table_desc()
                .unwrap();
            let path = relation_path();
            let relation = Relation::new(schema.id(), 42, path.clone(), desc.as_ref())
                .await
                .unwrap();
            for xid in 1..=8 {
                relation
                    .write_value(i32_bytes(1), i32_bytes(xid), xid as u64)
                    .await
                    .unwrap();
            }
            let next = relation
                .inner
                .next_tuple_id
                .load(std::sync::atomic::Ordering::Acquire);
            relation.close().await.unwrap();
            assert!(relation
                .write_value(i32_bytes(2), i32_bytes(2), 9)
                .await
                .is_err());
            drop(relation);
            let reopened = Relation::new(schema.id(), 42, path.clone(), desc.as_ref())
                .await
                .unwrap();
            assert_eq!(
                reopened
                    .inner
                    .next_tuple_id
                    .load(std::sync::atomic::Ordering::Acquire),
                next
            );
            let row = reopened
                .inner
                .index
                .get(&KeyTuple::from(i32_bytes(1)))
                .await
                .unwrap()
                .unwrap();
            let (_, authoritative) = row
                .read_detailed(&Snapshot::from(RunningXList::new(8, vec![])))
                .await
                .unwrap()
                .unwrap();
            // Full payload proves the checkpoint loaded; file rebuild produces metadata-only rows.
            assert!(authoritative);
            let manifest = format!(
                "{}.index-manifest",
                reopened.inner.key_file.path().display()
            );
            let fs = mudu_sys::default_sys_io_context().fs();
            let marker = fs
                .open(std::path::Path::new(&manifest), FileOptions::read_only())
                .await
                .unwrap();
            assert_eq!(marker.read_exact_at(0, 1).await.unwrap(), vec![0]);
            assert_eq!(
                reopened
                    .visible_value(
                        &KeyTuple::from(i32_bytes(1)),
                        &WorkerSnapshot::latest_committed()
                    )
                    .await
                    .unwrap(),
                Some(i32_bytes(8))
            );
        })
        .await;
}

#[tokio::test]
async fn corrupt_manifest_falls_back_to_authoritative_rebuild() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let schema = test_schema();
            let desc = TableInfo::new(schema.clone())
                .unwrap()
                .table_desc()
                .unwrap();
            let path = relation_path();
            let relation = Relation::new(schema.id(), 43, path.clone(), desc.as_ref())
                .await
                .unwrap();
            relation
                .write_value(i32_bytes(1), i32_bytes(5), 1)
                .await
                .unwrap();
            let manifest = format!(
                "{}.index-manifest",
                relation.inner.key_file.path().display()
            );
            relation.close().await.unwrap();
            drop(relation);
            let fs = mudu_sys::default_sys_io_context().fs();
            let file = fs
                .open(
                    std::path::Path::new(&manifest),
                    FileOptions::read_write_create(),
                )
                .await
                .unwrap();
            file.write_all_at(20, b"broken").await.unwrap();
            file.fsync().await.unwrap();
            let reopened = Relation::new(schema.id(), 43, path, desc.as_ref())
                .await
                .unwrap();
            assert_eq!(
                reopened
                    .visible_value(
                        &KeyTuple::from(i32_bytes(1)),
                        &WorkerSnapshot::latest_committed()
                    )
                    .await
                    .unwrap(),
                Some(i32_bytes(5))
            );
        })
        .await;
}

#[tokio::test]
async fn incomplete_write_never_publishes_a_clean_checkpoint() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let schema = test_schema();
            let desc = TableInfo::new(schema.clone())
                .unwrap()
                .table_desc()
                .unwrap();
            let relation = Relation::new(schema.id(), 44, relation_path(), desc.as_ref())
                .await
                .unwrap();
            relation
                .inner
                .cache_tainted
                .store(true, std::sync::atomic::Ordering::Release);
            assert!(relation.close().await.is_err());
            let manifest = format!(
                "{}.index-manifest",
                relation.inner.key_file.path().display()
            );
            let file = mudu_sys::default_sys_io_context()
                .fs()
                .open(std::path::Path::new(&manifest), FileOptions::read_only())
                .await
                .unwrap();
            assert_eq!(file.read_exact_at(0, 1).await.unwrap(), vec![0]);
        })
        .await;
}

#[tokio::test]
async fn online_checkpoint_stays_open_invalidates_before_writes_and_reopens() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let schema = test_schema();
            let desc = TableInfo::new(schema.clone())
                .unwrap()
                .table_desc()
                .unwrap();
            let path = relation_path();
            let relation = Relation::new(schema.id(), 45, path.clone(), desc.as_ref())
                .await
                .unwrap();
            relation
                .write_value(i32_bytes(1), i32_bytes(1), 1)
                .await
                .unwrap();
            relation.checkpoint().await.unwrap();
            let manifest = format!(
                "{}.index-manifest",
                relation.inner.key_file.path().display()
            );
            let file = mudu_sys::default_sys_io_context()
                .fs()
                .open(std::path::Path::new(&manifest), FileOptions::read_only())
                .await
                .unwrap();
            assert_eq!(file.read_exact_at(0, 1).await.unwrap(), vec![1]);
            assert_eq!(
                relation
                    .visible_value(
                        &KeyTuple::from(i32_bytes(1)),
                        &WorkerSnapshot::latest_committed()
                    )
                    .await
                    .unwrap(),
                Some(i32_bytes(1))
            );
            assert_eq!(file.read_exact_at(0, 1).await.unwrap(), vec![1]);
            relation
                .write_value(i32_bytes(1), i32_bytes(2), 2)
                .await
                .unwrap();
            assert_eq!(file.read_exact_at(0, 1).await.unwrap(), vec![0]);
            relation.checkpoint().await.unwrap();
            // No close: the online checkpoint alone must suffice for a clean reopen.
            drop(relation);
            let reopened = Relation::new(schema.id(), 45, path, desc.as_ref())
                .await
                .unwrap();
            let row = reopened
                .inner
                .index
                .get(&KeyTuple::from(i32_bytes(1)))
                .await
                .unwrap()
                .unwrap();
            let (_, payload) = row
                .read_detailed(&Snapshot::from(RunningXList::new(2, vec![])))
                .await
                .unwrap()
                .unwrap();
            assert!(
                payload,
                "checkpoint must restore full row images rather than rebuild metadata"
            );
            assert_eq!(
                reopened
                    .visible_value(
                        &KeyTuple::from(i32_bytes(1)),
                        &WorkerSnapshot::latest_committed()
                    )
                    .await
                    .unwrap(),
                Some(i32_bytes(2))
            );
            reopened.maintain_index().await.unwrap();
            reopened.checkpoint().await.unwrap();
            reopened
                .write_value(i32_bytes(2), i32_bytes(3), 3)
                .await
                .unwrap();
            reopened.close().await.unwrap();
            assert!(reopened.checkpoint().await.is_err());
            assert!(reopened.maintain_index().await.is_err());
            assert!(reopened
                .write_rows_delta(desc.as_ref(), &[], 4)
                .await
                .is_err());
        })
        .await;
}

#[tokio::test]
async fn checkpoint_barrier_waits_for_writes_and_cancellation_keeps_relation_open() {
    tokio::task::LocalSet::new()
        .run_until(async {
            use futures::{poll, FutureExt};
            let schema = test_schema();
            let desc = TableInfo::new(schema.clone())
                .unwrap()
                .table_desc()
                .unwrap();
            let relation = Relation::new(schema.id(), 46, relation_path(), desc.as_ref())
                .await
                .unwrap();
            let writer = relation.inner.lifecycle.read().await;
            let mut checkpoint = relation.checkpoint().boxed();
            assert!(poll!(&mut checkpoint).is_pending());
            drop(checkpoint);
            drop(writer);
            relation
                .write_value(i32_bytes(1), i32_bytes(2), 1)
                .await
                .unwrap();
            relation.checkpoint().await.unwrap();
            relation
                .inner
                .cache_tainted
                .store(true, std::sync::atomic::Ordering::Release);
            assert!(relation.checkpoint().await.is_err());
            let manifest = format!(
                "{}.index-manifest",
                relation.inner.key_file.path().display()
            );
            let file = mudu_sys::default_sys_io_context()
                .fs()
                .open(std::path::Path::new(&manifest), FileOptions::read_only())
                .await
                .unwrap();
            assert_eq!(file.read_exact_at(0, 1).await.unwrap(), vec![0]);
        })
        .await;
}
