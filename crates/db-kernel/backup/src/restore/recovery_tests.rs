#![allow(clippy::unwrap_used)]
use super::*;
use crate::backup::tests::test_worker_runtime;
use wal::worker_log::WorkerLogBackend;

#[test]
fn restored_wal_preserves_row_timestamps_tombstones_and_empty_row_clocks() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("restore-clocks-{}", mudu_utils::oid::gen_oid()))
            .to_string_lossy()
            .into_owned();
        let worker = test_worker_runtime(&dir, None).await;
        worker.initialize().await.unwrap();
        let epoch = catalog::contract::epoch_oracle::EPOCH_XID_BASE + 7;
        worker
            .replay_log_batch(XLBatch::new(vec![
                XLEntry {
                    xid: 5,
                    commit_epoch: None,
                    ops: vec![TxOp::Write(XLWrite::Insert(XLInsert {
                        table_id: 0,
                        partition_id: 0,
                        tuple_id: 0,
                        key: b"live".to_vec(),
                        value: b"value".to_vec(),
                    }))],
                },
                XLEntry {
                    xid: 6,
                    commit_epoch: Some(epoch),
                    ops: vec![TxOp::Write(XLWrite::Delete(XLDelete {
                        table_id: 0,
                        partition_id: 0,
                        tuple_id: 0,
                        key: b"deleted".to_vec(),
                    }))],
                },
                XLEntry {
                    xid: 90,
                    commit_epoch: Some(epoch + 10),
                    ops: vec![],
                },
            ]))
            .await
            .unwrap();
        let mut expected = worker.storage().dump_kv_latest().await;
        expected.sort();
        persist_kv_to_wal(&worker).await.unwrap();
        drop(worker);
        let reopened = test_worker_runtime(&dir, None).await;
        reopened.initialize().await.unwrap();
        let log = reopened.worker_log().unwrap().unwrap();
        let mut pending = vec![];
        let mut pending_start = None;
        for path in log.chunk_paths_sorted().await.unwrap() {
            let bytes = log.fs().read_all(&path).await.unwrap();
            let prefix = wal::worker_log::scan_valid_frame_prefix(&bytes);
            for batch in wal::xl_batch::decode_xl_batches_with_pending(
                &prefix.frames,
                &mut pending,
                &mut pending_start,
            )
            .unwrap()
            {
                reopened.replay_log_batch(batch).await.unwrap();
            }
        }
        assert!(pending.is_empty());
        let storage = reopened.storage();
        let mut actual = storage.dump_kv_latest().await;
        actual.sort();
        assert_eq!(actual, expected);
        let guard = storage.capture_participant_snapshot().await.unwrap();
        assert_eq!(
            guard.recovery_watermarks().unwrap().applied_epoch,
            epoch + 10
        );
        assert_eq!(reopened.worker_x_contract().latest_xid(), 90);
    })
    .unwrap();
}
