//! Durable lifecycle replay, including rejected commands.
#![allow(clippy::unwrap_used)]
use super::{service::tx_decision_descriptor, tx_machine::TxDecisionMachine};
use crate::log_stream::{frontier::encode, DurableStreamLog, StreamStateMachine};
use catalog::contract::tx_decision::{
    CoordinatorRef, TxDecision, TxDecisionError as E, TxDecisionStruct as R, TxParticipant,
};
use mudu_raft::stream::{GroupId, GroupIdobject};

#[test]
fn replay_preserves_state_and_rejected_results() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("decision-replay-{}", mudu_sys::random::uuid_v4()));
        let identity = GroupIdobject {
            cluster_id: 9,
            group_id: GroupId(1),
            generation: 3,
        };
        let coordinator = CoordinatorRef {
            cluster_id: 9,
            group_id: 1,
            group_generation: 3,
            tx_id: 42,
        };
        let commands = lifecycle_commands(&coordinator);
        let log = DurableStreamLog::open(dir.join("log"), identity, tx_decision_descriptor())
            .await
            .unwrap();
        let machine = TxDecisionMachine::open(&log, &dir.join("apply"))
            .await
            .unwrap();
        for (index, (command, expected)) in commands.iter().enumerate() {
            let (progress, _) = log
                .append(index as u128, encode(command).unwrap(), vec![])
                .await
                .unwrap();
            let (_, record) = log.record(progress.end_lsn).await.unwrap();
            machine.apply_once(index as u128, record).await.unwrap();
            assert_eq!(machine.result(progress.end_lsn).unwrap(), Some(*expected));
        }
        let state = machine.state(&coordinator).unwrap().unwrap();
        assert!(state.complete);
        drop(machine);
        drop(log);
        let log = DurableStreamLog::open(dir.join("log"), identity, tx_decision_descriptor())
            .await
            .unwrap();
        let machine = TxDecisionMachine::open(&log, &dir.join("apply"))
            .await
            .unwrap();
        assert_eq!(machine.state(&coordinator).unwrap(), Some(state));
        for (index, (_, expected)) in commands.iter().enumerate() {
            assert_eq!(machine.result(index as u64).unwrap(), Some(*expected));
        }
        // A durable but unapplied suffix is not exposed on recovery.
        let unknown = CoordinatorRef {
            tx_id: 43,
            ..coordinator
        };
        log.append(
            99,
            encode(&R::Begin {
                owner_term: 1,
                coordinator: unknown.clone(),
                participants: vec![TxParticipant {
                    group_id: 2,
                    group_generation: 1,
                    worker_index: 0,
                    partition_id: 8,
                }],
                snapshot_epoch: 5,
            })
            .unwrap(),
            vec![],
        )
        .await
        .unwrap();
        drop(machine);
        let machine = TxDecisionMachine::open(&log, &dir.join("apply"))
            .await
            .unwrap();
        assert!(machine.state(&unknown).unwrap().is_none());
    })
    .unwrap();
}

pub(super) fn lifecycle_commands(coordinator: &CoordinatorRef) -> Vec<(R, Result<(), E>)> {
    let participant = TxParticipant {
        group_id: 2,
        group_generation: 1,
        worker_index: 0,
        partition_id: 8,
    };
    [
        (
            R::Complete {
                coordinator: coordinator.clone(),
            },
            Err(E::MissingBegin),
        ),
        (
            R::Begin {
                owner_term: 1,
                coordinator: coordinator.clone(),
                participants: vec![participant.clone()],
                snapshot_epoch: 5,
            },
            Ok(()),
        ),
        (
            R::Decide {
                coordinator: coordinator.clone(),
                decision: TxDecision::Committed { commit_epoch: 7 },
            },
            Ok(()),
        ),
        (
            R::Decide {
                coordinator: coordinator.clone(),
                decision: TxDecision::Aborted,
            },
            Err(E::ConflictingDecision),
        ),
        (
            R::Complete {
                coordinator: coordinator.clone(),
            },
            Err(E::ParticipantsPending),
        ),
        (
            R::ParticipantDone {
                coordinator: coordinator.clone(),
                participant,
            },
            Ok(()),
        ),
        (
            R::Complete {
                coordinator: coordinator.clone(),
            },
            Ok(()),
        ),
    ]
    .to_vec()
}

#[test]
fn failed_frontier_publication_does_not_expose_state() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("decision-failure-{}", mudu_sys::random::uuid_v4()));
        let identity = GroupIdobject {
            cluster_id: 9,
            group_id: GroupId(1),
            generation: 3,
        };
        let coordinator = CoordinatorRef {
            cluster_id: 9,
            group_id: 1,
            group_generation: 3,
            tx_id: 42,
        };
        let log = DurableStreamLog::open(dir.join("log"), identity, tx_decision_descriptor())
            .await
            .unwrap();
        let apply = dir.join("apply");
        let machine = TxDecisionMachine::open(&log, &apply).await.unwrap();
        let command = lifecycle_commands(&coordinator)[1].0.clone();
        let (progress, _) = log
            .append(1, encode(&command).unwrap(), vec![])
            .await
            .unwrap();
        let (_, record) = log.record(progress.end_lsn).await.unwrap();
        let fs = mudu_sys::fs::tokio_fs();
        fs.remove_dir_all(&apply).await.unwrap();
        crate::durable_io::publish(fs.as_ref(), &apply, b"not a directory")
            .await
            .unwrap();
        assert!(machine.apply_once(1, record.clone()).await.is_err());
        assert!(machine.state(&coordinator).unwrap().is_none());
        assert!(machine.result(0).unwrap().is_none());
        assert!(machine.applied_frontier().await.unwrap().is_none());
        fs.remove_file_if_exists(&apply).await.unwrap();
        crate::durable_io::ensure_dir(fs.as_ref(), &apply)
            .await
            .unwrap();
        machine.apply_once(1, record.clone()).await.unwrap();
        machine.apply_once(1, record).await.unwrap();
        assert!(machine.state(&coordinator).unwrap().is_some());
        assert_eq!(machine.result(0).unwrap(), Some(Ok(())));
    })
    .unwrap();
}
