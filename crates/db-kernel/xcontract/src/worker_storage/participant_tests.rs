#![allow(clippy::unwrap_used)]
use super::*;
use catalog_testing::test_meta_mgr::TestMetaMgr;
use std::sync::Arc;
use wal::xl_data_op::{XLInsert, XLWrite};

fn block_on(future: impl std::future::Future<Output = ()> + 'static) {
    mudu_sys::task::async_::block_on_tokio_current_thread(future).unwrap();
}

fn storage() -> WorkerStorage {
    WorkerStorage::new(
        Arc::new(TestMetaMgr::new()),
        0,
        mudu_sys::env_var::temp_dir()
            .join(format!("participant-{}", mudu_utils::oid::gen_oid()))
            .to_string_lossy()
            .into_owned(),
    )
}
fn owner(tx_id: OID) -> catalog::contract::tx_decision::CoordinatorRef {
    catalog::contract::tx_decision::CoordinatorRef {
        cluster_id: 1,
        group_id: 1,
        group_generation: 1,
        tx_id,
    }
}
fn prepare(tx_id: OID, partition_id: OID) -> ParticipantPrepare {
    ParticipantPrepare {
        coordinator: owner(tx_id),
        partition_id,
        snapshot_epoch: 5,
        write_set: vec![XLWrite::Insert(XLInsert {
            table_id: 0,
            partition_id,
            tuple_id: 0,
            key: b"key".to_vec(),
            value: b"value".to_vec(),
        })],
    }
}
fn batch(op: TxOp) -> XLBatch {
    XLBatch::new(vec![XLEntry {
        xid: 1,
        ops: vec![op],
        commit_epoch: None,
    }])
}
fn decide(tx_id: OID, partition_id: OID, decision: TxDecision) -> TxOp {
    TxOp::ParticipantDecision(ParticipantDecision {
        coordinator: owner(tx_id),
        partition_id,
        decision,
        commit_ts: Default::default(),
    })
}

#[test]
fn prepare_replay_is_invisible_and_preserves_full_identity() {
    block_on(async {
        let storage = storage();
        let tx = (1_u128 << 100) | 1;
        for (tx_id, partition) in [(tx, 0), (tx, 7), (1, 8)] {
            let record = batch(TxOp::Prepare(prepare(tx_id, partition)));
            let bytes = rmp_serde::to_vec(&record).unwrap();
            storage
                .replay_batch(rmp_serde::from_slice(&bytes).unwrap())
                .await
                .unwrap();
            storage.replay_batch(record).await.unwrap();
            assert!(storage.participant_pending(partition).unwrap());
        }
        assert_eq!(storage.kv_get(b"key", None).await.unwrap(), None);
        storage
            .replay_batch(batch(decide(
                tx,
                0,
                TxDecision::Committed { commit_epoch: 7 },
            )))
            .await
            .unwrap();
        assert_eq!(
            storage.kv_get(b"key", None).await.unwrap(),
            Some(b"value".to_vec())
        );
        assert!(!storage.participant_pending(0).unwrap());
        assert!(storage.participant_pending(7).unwrap());
        assert!(storage.participant_pending(8).unwrap());
        assert!(storage.has_participant_history().unwrap());
    });
}

#[test]
fn decisions_reject_conflicts_and_abort_fences_late_prepare() {
    block_on(async {
        let storage = storage();
        let commit = decide(1, 0, TxDecision::Committed { commit_epoch: 7 });
        assert!(storage.replay_batch(batch(commit.clone())).await.is_err());
        let record = prepare(1, 0);
        storage
            .replay_batch(batch(TxOp::Prepare(record.clone())))
            .await
            .unwrap();
        let mut conflicting = record.clone();
        conflicting.snapshot_epoch += 1;
        assert!(storage
            .replay_batch(batch(TxOp::Prepare(conflicting)))
            .await
            .is_err());
        assert!(storage
            .replay_batch(batch(decide(
                1,
                0,
                TxDecision::Committed { commit_epoch: 4 }
            )))
            .await
            .is_err());
        storage.replay_batch(batch(commit.clone())).await.unwrap();
        storage.replay_batch(batch(commit)).await.unwrap();
        assert!(storage
            .replay_batch(batch(decide(1, 0, TxDecision::Aborted)))
            .await
            .is_err());
        storage
            .replay_batch(batch(decide(2, 0, TxDecision::Aborted)))
            .await
            .unwrap();
        assert!(storage
            .replay_batch(batch(TxOp::Prepare(prepare(2, 0))))
            .await
            .is_err());
        assert_eq!(
            storage.kv_get(b"key", None).await.unwrap(),
            Some(b"value".to_vec())
        );
    });
}

#[test]
fn replay_failure_retains_prepare_and_blocks_conflicting_writes() {
    block_on(async {
        let storage = storage();
        let mut record = prepare(3, 8);
        if let XLWrite::Insert(write) = &mut record.write_set[0] {
            write.table_id = 99;
        }
        storage
            .replay_batch(batch(TxOp::Prepare(record)))
            .await
            .unwrap();
        assert!(storage
            .replay_batch(batch(decide(
                3,
                8,
                TxDecision::Committed { commit_epoch: 7 }
            )))
            .await
            .is_err());
        assert!(storage.participant_pending(8).unwrap());
        assert!(storage
            .replay_batch(batch(decide(3, 8, TxDecision::Aborted)))
            .await
            .is_err());
        assert!(storage.ensure_partition_not_prepared(8).is_err());
        assert!(storage.ensure_partition_not_prepared(9).is_ok());
    });
}
