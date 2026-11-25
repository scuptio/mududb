//! Checkpoint plus suffix recovery and rejection of mismatched backup pieces.
#![allow(clippy::unwrap_used)]
use super::tx_machine_tests::lifecycle_commands;
use super::{service::tx_decision_descriptor, tx_machine::TxDecisionMachine};
use crate::log_stream::{frontier::encode, DurableStreamLog, StreamStateMachine};
use catalog::contract::tx_decision::{CoordinatorRef, TxDecisionStruct};
use mudu_raft::stream::{GroupId, GroupIdobject};
use std::path::PathBuf;

struct Fixture {
    dir: PathBuf,
    log: DurableStreamLog,
    machine: TxDecisionMachine,
    coordinator: CoordinatorRef,
}

impl Fixture {
    async fn new() -> Self {
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("decision-snapshot-{}", mudu_sys::random::uuid_v4()));
        let identity = GroupIdobject {
            cluster_id: 9,
            group_id: GroupId(1),
            generation: 3,
        };
        let log = DurableStreamLog::open(dir.join("log"), identity, tx_decision_descriptor())
            .await
            .unwrap();
        let machine = TxDecisionMachine::open(&log, &dir.join("apply"))
            .await
            .unwrap();
        let coordinator = CoordinatorRef {
            cluster_id: 9,
            group_id: 1,
            group_generation: 3,
            tx_id: (1_u128 << 100) | 42,
        };
        Self {
            dir,
            log,
            machine,
            coordinator,
        }
    }

    async fn append(&self, id: u128, command: &TxDecisionStruct, apply: bool) {
        let (prefix, _) = self
            .log
            .append(id, encode(command).unwrap(), vec![])
            .await
            .unwrap();
        if apply {
            let (_, record) = self.log.record(prefix.end_lsn).await.unwrap();
            self.machine.apply_once(id, record).await.unwrap();
        }
    }

    async fn begin(&self) {
        self.append(1, &lifecycle_commands(&self.coordinator)[1].0, true)
            .await;
    }

    fn snapshot(&self) -> PathBuf {
        self.dir.join("apply/tx_decision.snapshot")
    }
}

#[test]
fn checkpoint_and_suffix_restore_completed_pending_and_rejected_results() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        let commands = lifecycle_commands(&f.coordinator);
        for (index, (command, _)) in commands.iter().enumerate() {
            f.append(index as u128, command, true).await;
            if index == 3 {
                assert_eq!(f.machine.checkpoint().await.unwrap().unwrap().end_lsn, 3);
            }
        }
        // An isolated recovery test removes checkpoint-covered payloads to
        // prove recovery consumes the checkpoint rather than replaying them.
        // Production GC remains disabled until peer install and dedup exist.
        f.log.gc_prefix(4).await.unwrap();
        let pending = CoordinatorRef {
            tx_id: f.coordinator.tx_id + 1,
            ..f.coordinator.clone()
        };
        f.append(8, &lifecycle_commands(&pending)[1].0, true).await;
        let unknown = CoordinatorRef {
            tx_id: pending.tx_id + 1,
            ..pending.clone()
        };
        f.append(9, &lifecycle_commands(&unknown)[1].0, false).await;
        let log_dir = f.dir.join("log");
        let apply_dir = f.dir.join("apply");
        let identity = f.log.identity();
        let expected = f.machine.state(&f.coordinator).unwrap();
        drop(f.machine);
        drop(f.log);
        let log = DurableStreamLog::open(log_dir, identity, tx_decision_descriptor())
            .await
            .unwrap();
        let restored = TxDecisionMachine::open(&log, &apply_dir).await.unwrap();
        assert_eq!(restored.state(&f.coordinator).unwrap(), expected);
        assert_eq!(restored.pending().unwrap().len(), 1);
        assert!(restored
            .state(&pending)
            .unwrap()
            .unwrap()
            .decision
            .is_none());
        assert!(restored.state(&unknown).unwrap().is_none());
        for (index, (_, result)) in commands.iter().enumerate() {
            assert_eq!(restored.result(index as u64).unwrap(), Some(*result));
        }
    })
    .unwrap();
}

#[test]
fn snapshot_without_matching_apply_witness_is_rejected() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        f.begin().await;
        let frontier = f.dir.join("apply/tx_decision.frontier");
        let fs = mudu_sys::fs::tokio_fs();
        let old = fs.read_all(&frontier).await.unwrap();
        f.append(2, &lifecycle_commands(&f.coordinator)[2].0, true)
            .await;
        f.machine.checkpoint().await.unwrap();
        crate::durable_io::publish(fs.as_ref(), &frontier, &old)
            .await
            .unwrap();
        assert!(TxDecisionMachine::open(&f.log, &f.dir.join("apply"))
            .await
            .is_err());
        fs.remove_file_if_exists(&frontier).await.unwrap();
        assert!(TxDecisionMachine::open(&f.log, &f.dir.join("apply"))
            .await
            .is_err());
    })
    .unwrap();
}

#[test]
fn corrupted_and_foreign_snapshots_are_rejected() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        f.begin().await;
        f.machine.checkpoint().await.unwrap();
        let fs = mudu_sys::fs::tokio_fs();
        let bytes = fs.read_all(&f.snapshot()).await.unwrap();
        let foreign = GroupIdobject {
            generation: 4,
            ..f.log.identity()
        };
        let log = DurableStreamLog::open(f.dir.join("foreign"), foreign, tx_decision_descriptor())
            .await
            .unwrap();
        assert!(TxDecisionMachine::open(&log, &f.dir.join("apply"))
            .await
            .is_err());
        let mut corrupted = bytes.clone();
        *corrupted.last_mut().unwrap() ^= 1;
        crate::durable_io::publish(fs.as_ref(), &f.snapshot(), &corrupted)
            .await
            .unwrap();
        assert!(TxDecisionMachine::open(&f.log, &f.dir.join("apply"))
            .await
            .is_err());
        crate::durable_io::publish(fs.as_ref(), &f.snapshot(), &bytes)
            .await
            .unwrap();
        assert!(TxDecisionMachine::open(&f.log, &f.dir.join("apply"))
            .await
            .is_ok());
    })
    .unwrap();
}

#[test]
fn periodic_checkpoint_is_published_without_an_explicit_request() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        let begin = lifecycle_commands(&f.coordinator)[1].0.clone();
        for id in 0..256 {
            f.append(id, &begin, true).await;
        }
        assert!(mudu_sys::fs::tokio_fs()
            .path_exists(&f.snapshot())
            .await
            .unwrap());
        let restored = TxDecisionMachine::open(&f.log, &f.dir.join("apply"))
            .await
            .unwrap();
        assert_eq!(restored.result(255).unwrap(), Some(Ok(())));
        assert_eq!(restored.pending().unwrap().len(), 1);
    })
    .unwrap();
}

#[test]
fn automatic_checkpoint_failure_does_not_change_committed_results() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        let fs = mudu_sys::fs::tokio_fs();
        // Renaming a file over a directory fails at the publication boundary.
        fs.create_dir_all(&f.snapshot()).await.unwrap();
        let begin = lifecycle_commands(&f.coordinator)[1].0.clone();
        for id in 0..256 {
            f.append(id, &begin, true).await;
        }
        assert_eq!(f.machine.result(255).unwrap(), Some(Ok(())));
        assert!(f.machine.checkpoint().await.is_err());
        fs.remove_dir_all(&f.snapshot()).await.unwrap();
        assert_eq!(f.machine.checkpoint().await.unwrap().unwrap().end_lsn, 255);
        let restored = TxDecisionMachine::open(&f.log, &f.dir.join("apply"))
            .await
            .unwrap();
        assert_eq!(restored.result(255).unwrap(), Some(Ok(())));
    })
    .unwrap();
}

#[test]
fn exported_decision_image_matches_checkpoint_and_rejects_wrong_cut() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        assert!(!f.machine.snapshot_at(None).await.unwrap().is_empty());
        f.begin().await;
        let prefix = f.machine.applied_frontier().await.unwrap();
        let bytes = f.machine.snapshot_at(prefix.as_ref()).await.unwrap();
        assert!(f.machine.snapshot_at(None).await.is_err());
        let mut wrong = prefix.clone().unwrap();
        wrong.segment_digest[0] ^= 1;
        assert!(f.machine.snapshot_at(Some(&wrong)).await.is_err());
        f.machine.checkpoint().await.unwrap();
        assert_eq!(
            mudu_sys::fs::tokio_fs()
                .read_all(&f.snapshot())
                .await
                .unwrap(),
            bytes
        );
    })
    .unwrap();
}

mod restore;
