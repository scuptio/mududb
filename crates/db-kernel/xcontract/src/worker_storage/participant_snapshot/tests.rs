#![allow(clippy::unwrap_used)]
use super::*;
use catalog_testing::test_meta_mgr::TestMetaMgr;
use std::sync::Arc;
use wal::participant::{ParticipantDecision, ParticipantPrepare};
use wal::xl_data_op::{XLInsert, XLWrite};
use wal::{
    xl_batch::XLBatch,
    xl_entry::{TxOp, XLEntry},
};

fn storage() -> WorkerStorage {
    WorkerStorage::new(
        Arc::new(TestMetaMgr::new()),
        0,
        mudu_sys::env_var::temp_dir()
            .join(format!(
                "participant-snapshot-{}",
                mudu_utils::oid::gen_oid()
            ))
            .to_string_lossy()
            .into_owned(),
    )
}
fn prepare(partition: OID) -> ParticipantPrepare {
    ParticipantPrepare {
        coordinator: CoordinatorRef {
            cluster_id: 1,
            group_id: 2,
            group_generation: 3,
            tx_id: 1 << 100,
        },
        partition_id: partition,
        snapshot_epoch: 5,
        write_set: vec![XLWrite::Insert(XLInsert {
            table_id: 0,
            partition_id: partition,
            tuple_id: 0,
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        })],
    }
}
async fn apply(storage: &WorkerStorage, op: TxOp) {
    storage
        .replay_batch(XLBatch::new(vec![XLEntry {
            xid: 0,
            ops: vec![op],
            commit_epoch: None,
        }]))
        .await
        .unwrap();
}
fn decision(partition: OID, decision: TxDecision) -> ParticipantDecision {
    ParticipantDecision {
        coordinator: prepare(partition).coordinator,
        partition_id: partition,
        decision,
        commit_ts: Default::default(),
    }
}
fn run(future: impl std::future::Future<Output = ()> + 'static) {
    mudu_sys::task::async_::block_on_tokio_current_thread(future).unwrap();
}

#[test]
fn snapshot_preserves_invisible_prepares_final_results_and_abort_tombstones() {
    run(async {
        let source = storage();
        apply(&source, TxOp::Prepare(prepare(0))).await;
        apply(&source, TxOp::Prepare(prepare(7))).await;
        let commit = decision(0, TxDecision::Committed { commit_epoch: 8 });
        apply(&source, TxOp::ParticipantDecision(commit.clone())).await;
        apply(
            &source,
            TxOp::ParticipantDecision(decision(9, TxDecision::Aborted)),
        )
        .await;
        let guard = source.capture_participant_snapshot().await.unwrap();
        let bytes = rmp_serde::to_vec(guard.snapshot()).unwrap();
        let snapshot: ParticipantSnapshot = rmp_serde::from_slice(&bytes).unwrap();
        let target = storage();
        target
            .restore_kv_latest(source.dump_kv_latest().await)
            .await
            .unwrap();
        target
            .restore_participant_snapshot(&snapshot)
            .await
            .unwrap();
        target
            .restore_participant_snapshot(&snapshot)
            .await
            .unwrap();
        assert!(target.check_participant_decision(&commit).unwrap());
        assert!(target.check_participant_prepare(&prepare(7)).unwrap());
        assert!(target.participant_pending(7).unwrap());
        assert!(!target.participant_pending(0).unwrap());
        assert!(target.check_participant_prepare(&prepare(9)).is_err());
        assert!(target
            .check_participant_decision(&decision(0, TxDecision::Aborted))
            .is_err());
        assert!(target.has_participant_history().unwrap());
        assert_eq!(
            target.kv_get(b"key", None).await.unwrap(),
            Some(b"value".to_vec())
        );
        apply(
            &target,
            TxOp::ParticipantDecision(decision(7, TxDecision::Aborted)),
        )
        .await;
        assert!(!target.participant_pending(7).unwrap());
    });
}

#[test]
fn capture_fences_new_wal_intent_and_cancelled_apply_until_guard_drops() {
    run(async {
        let store = storage();
        let guard = store.capture_participant_snapshot().await.unwrap();
        assert!(store.reserve_participant_wal(0).is_err());
        let pending_apply = store.apply_participant_op(TxOp::Prepare(prepare(0)));
        assert!(
            mudu_sys::timeout(std::time::Duration::from_millis(10), pending_apply)
                .await
                .is_none()
        );
        assert!(guard.snapshot().entries.is_empty());
        drop(guard);
        store.reserve_participant_wal(0).unwrap();
        assert!(store.capture_participant_snapshot().await.is_err());
        apply(&store, TxOp::Prepare(prepare(0))).await;
        assert_eq!(
            store
                .capture_participant_snapshot()
                .await
                .unwrap()
                .snapshot()
                .entries
                .len(),
            1
        );
    });
}

#[test]
fn invalid_snapshot_never_partially_changes_the_destination() {
    run(async {
        let source = storage();
        apply(&source, TxOp::Prepare(prepare(0))).await;
        let image = source
            .capture_participant_snapshot()
            .await
            .unwrap()
            .snapshot()
            .clone();
        let target = storage();
        let mut invalid = image.clone();
        invalid.entries.push(invalid.entries[0].clone());
        assert!(target.restore_participant_snapshot(&invalid).await.is_err());
        assert!(!target.has_participant_history().unwrap());
        invalid = image.clone();
        invalid.entries[0].1.decision = Some(TxDecision::Committed { commit_epoch: 8 });
        assert!(target.restore_participant_snapshot(&invalid).await.is_err());
        invalid.entries[0].1.applied = true;
        invalid.entries[0].0 .1 = 99;
        assert!(target.restore_participant_snapshot(&invalid).await.is_err());
        invalid = image.clone();
        invalid.version = 2;
        assert!(target.restore_participant_snapshot(&invalid).await.is_err());
        target.restore_participant_snapshot(&image).await.unwrap();
        apply(
            &target,
            TxOp::ParticipantDecision(decision(0, TxDecision::Aborted)),
        )
        .await;
        assert!(target.restore_participant_snapshot(&image).await.is_err());
        assert!(target.check_participant_prepare(&prepare(0)).is_err());
    });
}

#[test]
fn partially_applied_commit_is_not_a_valid_snapshot_cut() {
    run(async {
        let store = storage();
        apply(&store, TxOp::Prepare(prepare(0))).await;
        store
            .participant_states
            .lock()
            .unwrap()
            .get_mut(&(prepare(0).coordinator, 0))
            .unwrap()
            .decision = Some(TxDecision::Committed { commit_epoch: 8 });
        assert!(store.capture_participant_snapshot().await.is_err());
        // Failure did not leak the capture flag or retain the apply lock.
        store.reserve_participant_wal(0).unwrap();
        apply(
            &store,
            TxOp::ParticipantDecision(decision(0, TxDecision::Committed { commit_epoch: 8 })),
        )
        .await;
        assert!(store.capture_participant_snapshot().await.is_ok());
    });
}

#[test]
fn capture_excludes_ordinary_commit_and_wal_replay() {
    run(async {
        for replay in [false, true] {
            let store = storage();
            let batch = XLBatch::new(vec![XLEntry {
                xid: 5,
                ops: vec![TxOp::Write(prepare(0).write_set[0].clone())],
                commit_epoch: None,
            }]);
            let guard = store.capture_participant_snapshot().await.unwrap();
            let pending = async {
                if replay {
                    store.replay_batch(batch.clone()).await
                } else {
                    let prepared = store.prepare_worker_kv_autocommit(
                        5,
                        b"key".to_vec(),
                        Some(b"value".to_vec()),
                        batch.clone(),
                    );
                    store.apply_prepared_commit_async(prepared).await
                }
            };
            assert!(
                mudu_sys::timeout(std::time::Duration::from_millis(10), pending)
                    .await
                    .is_none()
            );
            assert!(store.dump_kv_latest().await.is_empty());
            drop(guard);
            store.replay_batch(batch).await.unwrap();
            assert_eq!(
                store.kv_get(b"key", None).await.unwrap(),
                Some(b"value".to_vec())
            );
        }
    });
}

#[test]
fn cancelled_capture_does_not_block_later_apply_or_capture() {
    run(async {
        let store = storage();
        // Represents an admitted ordinary apply which has not finished yet.
        let writer = store.snapshot_apply.read().await;
        assert!(mudu_sys::timeout(
            std::time::Duration::from_millis(10),
            store.capture_participant_snapshot()
        )
        .await
        .is_none());
        drop(writer);
        apply(&store, TxOp::Prepare(prepare(0))).await;
        assert_eq!(
            store
                .capture_participant_snapshot()
                .await
                .unwrap()
                .snapshot()
                .entries
                .len(),
            1
        );
    });
}

#[test]
fn snapshot_install_retains_exclusive_apply_without_relocking() {
    run(async {
        let store = storage();
        let guard = store.capture_participant_snapshot().await.unwrap();
        let image = vec![(b"key".to_vec(), 5, Some(b"value".to_vec()))];
        assert!(mudu_sys::timeout(
            std::time::Duration::from_millis(10),
            store.restore_kv_latest(image.clone())
        )
        .await
        .is_none());
        assert!(store.dump_kv_latest().await.is_empty());
        mudu_sys::timeout(
            std::time::Duration::from_secs(1),
            guard.restore_kv_latest(image),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            store.kv_get(b"key", None).await.unwrap(),
            Some(b"value".to_vec())
        );
        drop(guard);
        store.restore_kv_latest(vec![]).await.unwrap();
        assert!(store.dump_kv_latest().await.is_empty());
    });
}

#[test]
fn failed_partial_row_apply_cannot_be_certified_by_a_later_success() {
    run(async {
        let store = storage();
        let good = prepare(0).write_set[0].clone();
        let bad = XLWrite::Insert(XLInsert {
            table_id: 999,
            partition_id: 0,
            tuple_id: 0,
            key: b"missing-table".to_vec(),
            value: b"value".to_vec(),
        });
        let batch = XLBatch::new(vec![XLEntry {
            xid: 5,
            ops: vec![TxOp::Write(good), TxOp::Write(bad)],
            commit_epoch: None,
        }]);
        assert!(store.replay_batch(batch).await.is_err());
        assert_eq!(
            store.kv_get(b"key", None).await.unwrap(),
            Some(b"value".to_vec())
        );
        assert!(store.capture_participant_snapshot().await.is_err());
        apply(
            &store,
            TxOp::ParticipantDecision(decision(7, TxDecision::Aborted)),
        )
        .await;
        assert!(store.capture_participant_snapshot().await.is_err());
    });
}

#[test]
fn cancellation_after_a_row_mutation_blocks_snapshot_capture() {
    run(async {
        let store = storage();
        let interrupted = async {
            let attempt = store.begin_snapshot_apply().await.unwrap();
            store
                .worker_put_local(b"key".to_vec(), b"partial".to_vec(), 5)
                .await
                .unwrap();
            std::future::pending::<()>().await;
            attempt.finish(Ok(()))
        };
        assert!(
            mudu_sys::timeout(std::time::Duration::from_millis(10), interrupted)
                .await
                .is_none()
        );
        assert_eq!(
            store.kv_get(b"key", None).await.unwrap(),
            Some(b"partial".to_vec())
        );
        assert!(store.capture_participant_snapshot().await.is_err());
    });
}

#[test]
fn snapshot_requires_an_exact_unmodified_applied_wal_cut() {
    run(async {
        let store = storage();
        let batch = XLBatch::new(vec![XLEntry {
            xid: 5,
            commit_epoch: None,
            ops: vec![TxOp::Write(prepare(0).write_set[0].clone())],
        }]);
        let prepared =
            store.prepare_worker_kv_autocommit(5, b"key".to_vec(), Some(b"value".to_vec()), batch);
        store
            .apply_prepared_commit_at_async(prepared, 12)
            .await
            .unwrap();
        let guard = store.capture_participant_snapshot().await.unwrap();
        guard.require_wal_resume(12).unwrap();
        assert!(guard.require_wal_resume(11).is_err());
        assert!(guard.require_wal_resume(13).is_err());
        guard.restore_kv_latest(vec![]).await.unwrap();
        assert!(guard.require_wal_resume(12).is_err());
        drop(guard);
        apply(
            &store,
            TxOp::ParticipantDecision(decision(7, TxDecision::Aborted)),
        )
        .await;
        assert!(store
            .capture_participant_snapshot()
            .await
            .unwrap()
            .require_wal_resume(12)
            .is_err());
    });
}

#[test]
fn concurrent_apply_cannot_certify_a_serial_wal_cut() {
    run(async {
        let store = storage();
        for newest_finishes_last in [false, true] {
            let first = store.begin_snapshot_apply().await.unwrap();
            let second = store.begin_snapshot_apply().await.unwrap();
            if newest_finishes_last {
                first.finish_at(Ok(()), Some(30)).unwrap();
                second.finish_at(Ok(()), Some(20)).unwrap();
            } else {
                second.finish_at(Ok(()), Some(20)).unwrap();
                first.finish_at(Ok(()), Some(30)).unwrap();
            }
            let guard = store.capture_participant_snapshot().await.unwrap();
            assert!(guard.require_wal_resume(20).is_err());
            assert!(guard.require_wal_resume(30).is_err());
        }
        let serial = store.begin_snapshot_apply().await.unwrap();
        serial.finish_at(Ok(()), Some(40)).unwrap();
        let guard = store.capture_participant_snapshot().await.unwrap();
        guard.require_wal_resume(40).unwrap();
    });
}

#[test]
fn cancelled_install_after_kv_restore_fences_later_snapshots() {
    run(async {
        let store = storage();
        let install = async {
            let guard = store.capture_participant_snapshot().await.unwrap();
            let attempt = guard.begin_install().unwrap();
            guard
                .restore_kv_latest(vec![(b"key".to_vec(), 5, Some(b"partial".to_vec()))])
                .await
                .unwrap();
            std::future::pending::<()>().await;
            attempt.complete();
        };
        assert!(
            mudu_sys::timeout(std::time::Duration::from_millis(10), install)
                .await
                .is_none()
        );
        assert_eq!(
            store.kv_get(b"key", None).await.unwrap(),
            Some(b"partial".to_vec())
        );
        assert!(store.capture_participant_snapshot().await.is_err());
        // A later successful individual restore cannot certify the rest of
        // the interrupted file/catalog/WAL installation.
        store.restore_kv_latest(vec![]).await.unwrap();
        assert!(store.capture_participant_snapshot().await.is_err());
    });
}

#[test]
fn completed_install_invalidates_old_cut_without_fencing_capture() {
    run(async {
        let store = storage();
        let apply = store.begin_snapshot_apply().await.unwrap();
        apply.finish_at(Ok(()), Some(9)).unwrap();
        let guard = store.capture_participant_snapshot().await.unwrap();
        guard.require_wal_resume(9).unwrap();
        let install = guard.begin_install().unwrap();
        guard.restore_kv_latest(vec![]).await.unwrap();
        install.complete();
        assert!(guard.require_wal_resume(9).is_err());
        drop(guard);
        assert!(store.capture_participant_snapshot().await.is_ok());
    });
}

#[test]
fn failed_relation_reload_fences_capture_even_after_outer_completion() {
    run(async {
        let store = storage();
        // A regular file where the relation root must be makes reopening
        // fail after it has cleared the old in-memory relation map.
        mudu_sys::fs::sync::write(store.relation_path(), b"not a directory").unwrap();
        let guard = store.capture_participant_snapshot().await.unwrap();
        let install = guard.begin_install().unwrap();
        assert!(guard.reload_relations_from_disk().await.is_err());
        install.complete();
        drop(guard);
        mudu_sys::fs::sync::sync_remove_file(store.relation_path()).unwrap();
        assert!(store.capture_participant_snapshot().await.is_err());
    });
}

#[test]
fn recovery_watermarks_follow_successful_rows_and_participant_decisions() {
    run(async {
        let store = storage();
        store
            .replay_batch(XLBatch::new(vec![XLEntry {
                xid: 91,
                commit_epoch: Some(6),
                ops: vec![],
            }]))
            .await
            .unwrap();
        apply(&store, TxOp::Prepare(prepare(0))).await;
        apply(
            &store,
            TxOp::ParticipantDecision(decision(0, TxDecision::Committed { commit_epoch: 12 })),
        )
        .await;
        let guard = store.capture_participant_snapshot().await.unwrap();
        let water = guard.recovery_watermarks().unwrap();
        assert_eq!(water.max_xid, 91);
        assert_eq!(water.applied_epoch, 12);
        let image = guard.snapshot().clone();
        let target = storage();
        let target_guard = target.capture_participant_snapshot().await.unwrap();
        target_guard.restore_participants(&image).unwrap();
        target_guard.restore_recovery_watermarks(water).unwrap();
        assert_eq!(target_guard.recovery_watermarks().unwrap(), water);
        assert!(target_guard.require_wal_resume(1).is_err());
    });
}

#[test]
fn failed_batch_does_not_advance_recovery_clocks() {
    run(async {
        let store = storage();
        let batch = XLBatch::new(vec![XLEntry {
            xid: 999,
            commit_epoch: Some(777),
            ops: vec![TxOp::Write(XLWrite::Insert(XLInsert {
                table_id: 999,
                partition_id: 0,
                tuple_id: 0,
                key: vec![1],
                value: vec![2],
            }))],
        }]);
        assert!(store.replay_batch(batch).await.is_err());
        assert_eq!(
            store.snapshot_cut.lock().unwrap().watermarks,
            Default::default()
        );
        assert!(store.capture_participant_snapshot().await.is_err());
    });
}
