#![allow(clippy::unwrap_used, clippy::panic)]
use super::*;
use catalog::contract::tx_decision::TxDecision;
use catalog_testing::test_meta_mgr::TestMetaMgr;
use mudu_sys::tokio::sync::{mpsc, oneshot};
use wal::participant::{ParticipantDecision, ParticipantPrepare};

fn owner() -> catalog::contract::tx_decision::CoordinatorRef {
    catalog::contract::tx_decision::CoordinatorRef {
        cluster_id: 1,
        group_id: 1,
        group_generation: 1,
        tx_id: 1,
    }
}
struct Leader;
impl RaftShareView for Leader {
    fn is_serving_leader(&self) -> bool {
        true
    }
    fn leader_hint(&self) -> Option<mudu_contract::protocol::LeaderHint> {
        None
    }
    fn leader_node_id(&self) -> Option<u64> {
        Some(1)
    }
    fn term_source(&self) -> Arc<AtomicU64> {
        Arc::new(AtomicU64::new(1))
    }
}
struct Sequencer(mpsc::UnboundedSender<(PreparedWorkerCommit, oneshot::Sender<RS<()>>)>);
impl CommitSequencer for Sequencer {
    fn stage(
        &self,
        prepared: PreparedWorkerCommit,
        _: usize,
        _: usize,
        _: wal::lsn::LSN,
    ) -> RS<oneshot::Receiver<RS<()>>> {
        let (done, wait) = oneshot::channel();
        self.0
            .send((prepared, done))
            .map_err(|_| mudu_error!(ErrorCode::ChannelClosed, "test sequencer stopped"))?;
        Ok(wait)
    }
}

#[test]
fn participant_waits_for_group_commit_and_replays_after_reopen() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let path = mudu_sys::env_var::temp_dir().join(format!("participant-quorum-{}", gen_oid()));
        let layout = WorkerLogLayout::new(path, gen_oid(), 4096).unwrap();
        let log = ChunkedWorkerLogBackend::new(layout.clone()).await.unwrap();
        let contract = Arc::new(
            WorkerXContract::with_log(Arc::new(TestMetaMgr::new()), Some(log.clone())).unwrap(),
        );
        contract.set_raft_share(Arc::new(Leader)).unwrap();
        let (send, mut recv) = mpsc::unbounded_channel();
        contract
            .set_commit_sequencer(Arc::new(Sequencer(send)))
            .unwrap();
        let prepare = ParticipantPrepare {
            coordinator: owner(),
            partition_id: 0,
            snapshot_epoch: 5,
            write_set: vec![XLWrite::Insert(XLInsert {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key: b"k".to_vec(),
                value: b"v".to_vec(),
            })],
        };
        let op = TxOp::Prepare(prepare);
        let submit = contract.persist_participant_op(op.clone());
        tokio::pin!(submit);
        let (prepared, done) = tokio::select! {
            result = &mut submit => { result.unwrap(); unreachable!("ack before quorum") },
            staged = recv.recv() => staged.unwrap(),
        };
        assert!(contract.storage.participant_pending(0).unwrap());
        assert!(!contract
            .storage
            .check_participant_prepare(match &op {
                TxOp::Prepare(record) => record,
                _ => unreachable!(),
            })
            .unwrap());
        contract
            .storage
            .apply_prepared_commit_async(prepared)
            .await
            .unwrap();
        done.send(Ok(())).unwrap();
        submit.await.unwrap();
        assert!(contract.storage.participant_pending(0).unwrap());
        assert_eq!(contract.worker_get_async(b"k").await.unwrap(), None);
        // Recovery consumes actual WAL bytes, including the full prepare body.
        let bytes = mudu_sys::fs::sync::read(layout.chunk_path(0)).unwrap();
        let frames = wal::worker_log::decode_frames(&bytes).unwrap();
        let recovered = WorkerXContract::with_log(Arc::new(TestMetaMgr::new()), None).unwrap();
        for batch in wal::xl_batch::decode_xl_batches(&frames).unwrap() {
            recovered.replay_worker_log_batch(batch).await.unwrap();
        }
        assert!(recovered.storage.participant_pending(0).unwrap());
        assert_eq!(recovered.worker_get_async(b"k").await.unwrap(), None);
        recovered
            .storage
            .replay_batch(XLBatch::new(vec![XLEntry {
                xid: 1,
                ops: vec![TxOp::ParticipantDecision(ParticipantDecision {
                    coordinator: owner(),
                    partition_id: 0,
                    decision: TxDecision::Committed { commit_epoch: 7 },
                    commit_ts: Default::default(),
                })],
                commit_epoch: Some(7),
            }]))
            .await
            .unwrap();
        assert_eq!(
            recovered.worker_get_async(b"k").await.unwrap(),
            Some(b"v".to_vec())
        );
    })
    .unwrap();
}

#[test]
fn cancelled_participant_submission_keeps_its_fence_until_apply() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let path = mudu_sys::env_var::temp_dir().join(format!("participant-cancel-{}", gen_oid()));
        let log =
            ChunkedWorkerLogBackend::new(WorkerLogLayout::new(path, gen_oid(), 4096).unwrap())
                .await
                .unwrap();
        let contract = WorkerXContract::with_log(Arc::new(TestMetaMgr::new()), Some(log)).unwrap();
        contract.set_raft_share(Arc::new(Leader)).unwrap();
        let (send, mut recv) = mpsc::unbounded_channel();
        contract
            .set_commit_sequencer(Arc::new(Sequencer(send)))
            .unwrap();
        let decision = ParticipantDecision {
            coordinator: owner(),
            partition_id: 9,
            decision: TxDecision::Aborted,
            commit_ts: Default::default(),
        };
        let mut submit =
            Box::pin(contract.persist_participant_op(TxOp::ParticipantDecision(decision.clone())));
        let (prepared, done) = tokio::select! {
            result = &mut submit => { result.unwrap(); unreachable!("ack before quorum") },
            staged = recv.recv() => staged.unwrap(),
        };
        drop(submit);
        assert!(contract.storage.participant_pending(9).unwrap());
        assert!(contract
            .persist_participant_op(TxOp::ParticipantDecision(decision.clone()))
            .await
            .is_err());
        contract
            .storage
            .apply_prepared_commit_async(prepared)
            .await
            .unwrap();
        assert!(done.send(Ok(())).is_err());
        assert!(!contract.storage.participant_pending(9).unwrap());
        assert!(contract
            .storage
            .check_participant_decision(&decision)
            .unwrap());
    })
    .unwrap();
}
