#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Feature-agnostic tests for the relation maintenance operations
//! (checkpoint / close / maintain_index): the lifecycle gating and retry
//! behavior hold with either index backend. The paged-index checkpoint
//! behavior itself lives in tests_checkpoint.rs.

use super::tests::{i32_bytes, relation_path, test_schema};
use super::*;
use catalog::contract::table_desc::TableDesc;
use catalog::contract::table_info::TableInfo;
use mudu_sys::task::async_::block_on_tokio_current_thread;
use std::sync::atomic::Ordering;

/// A fresh table id plus its descriptor; share the pair across (re)opens —
/// the relation files are keyed by table id, so a reopen must reuse it.
fn test_table() -> (OID, Arc<TableDesc>) {
    let schema = test_schema();
    let table_id = schema.id();
    let desc = TableInfo::new(schema).unwrap().table_desc().unwrap();
    (table_id, desc)
}

async fn open_test_relation(table_id: OID, desc: &TableDesc, path: &str) -> Relation {
    Relation::new(table_id, 7, path.to_string(), desc)
        .await
        .unwrap()
}

#[test]
fn checkpoint_then_writes_continue() {
    block_on_tokio_current_thread(async move {
        let (table_id, desc) = test_table();
        let path = relation_path();
        let relation = open_test_relation(table_id, &desc, &path).await;
        relation
            .write_value(i32_bytes(1), i32_bytes(11), 1)
            .await
            .unwrap();
        relation.checkpoint().await.unwrap();
        relation.maintain_index().await.unwrap();
        relation
            .write_value(i32_bytes(1), i32_bytes(12), 2)
            .await
            .unwrap();
        relation
            .write_value(i32_bytes(2), i32_bytes(22), 3)
            .await
            .unwrap();
        assert_eq!(
            relation
                .visible_value(
                    &KeyTuple::from(i32_bytes(1)),
                    &WorkerSnapshot::new(3, vec![])
                )
                .await
                .unwrap(),
            Some(i32_bytes(12))
        );
        assert_eq!(
            relation
                .visible_value(
                    &KeyTuple::from(i32_bytes(2)),
                    &WorkerSnapshot::new(3, vec![])
                )
                .await
                .unwrap(),
            Some(i32_bytes(22))
        );
        relation.close().await.unwrap();
    })
    .unwrap()
}

#[test]
fn operations_rejected_after_close() {
    block_on_tokio_current_thread(async move {
        let (table_id, desc) = test_table();
        let path = relation_path();
        let relation = open_test_relation(table_id, &desc, &path).await;
        relation
            .write_value(i32_bytes(1), i32_bytes(11), 1)
            .await
            .unwrap();
        relation.close().await.unwrap();

        let err = relation.checkpoint().await.unwrap_err();
        assert!(err.to_string().contains("closed"), "unexpected: {err}");
        let err = relation.maintain_index().await.unwrap_err();
        assert!(err.to_string().contains("closed"), "unexpected: {err}");
        assert!(relation
            .write_value(i32_bytes(2), i32_bytes(22), 2)
            .await
            .is_err());
    })
    .unwrap()
}

#[test]
fn reopen_after_close_recovers_data() {
    block_on_tokio_current_thread(async move {
        let (table_id, desc) = test_table();
        let path = relation_path();
        let relation = open_test_relation(table_id, &desc, &path).await;
        relation
            .write_value(i32_bytes(1), i32_bytes(11), 1)
            .await
            .unwrap();
        relation
            .write_value(i32_bytes(2), i32_bytes(22), 2)
            .await
            .unwrap();
        relation.close().await.unwrap();
        drop(relation);

        let reopened = open_test_relation(table_id, &desc, &path).await;
        let snapshot = WorkerSnapshot::latest_committed();
        assert_eq!(
            reopened
                .visible_value(&KeyTuple::from(i32_bytes(1)), &snapshot)
                .await
                .unwrap(),
            Some(i32_bytes(11))
        );
        assert_eq!(
            reopened
                .visible_value(&KeyTuple::from(i32_bytes(2)), &snapshot)
                .await
                .unwrap(),
            Some(i32_bytes(22))
        );
        reopened.close().await.unwrap();
    })
    .unwrap()
}

#[test]
fn tainted_cache_blocks_close_until_cleared() {
    block_on_tokio_current_thread(async move {
        let (table_id, desc) = test_table();
        let path = relation_path();
        let relation = open_test_relation(table_id, &desc, &path).await;
        relation
            .write_value(i32_bytes(1), i32_bytes(11), 1)
            .await
            .unwrap();

        relation.inner.cache_tainted.store(true, Ordering::Release);
        let err = relation.close().await.unwrap_err();
        assert!(
            err.to_string().contains("incomplete write"),
            "unexpected: {err}"
        );

        // A failed close can be retried once the taint is cleared.
        relation.inner.cache_tainted.store(false, Ordering::Release);
        relation.close().await.unwrap();
    })
    .unwrap()
}
