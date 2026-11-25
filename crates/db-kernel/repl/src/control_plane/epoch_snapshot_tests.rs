#![allow(clippy::unwrap_used)]
use super::{
    epoch_machine::EpochStreamMachine, epoch_snapshot::EpochImage,
    streams::epoch_stream_descriptor, EpochSnapshot,
};
use crate::log_stream::{
    frontier::{decode, encode},
    DurableStreamLog, FrontierStore, LoggedStreamAdapter, StreamAdapter, StreamStateMachine,
};
use catalog::contract::{
    epoch_op::EpochOp,
    epoch_oracle::{ClusterEpochState, EPOCH_XID_BASE},
};
use mudu_raft::stream::{GroupId, GroupIdobject};
use std::{
    path::PathBuf,
    sync::{atomic::Ordering, Arc},
};

fn identity() -> GroupIdobject {
    GroupIdobject {
        cluster_id: 7,
        group_id: GroupId(1),
        generation: 2,
    }
}
fn dir() -> PathBuf {
    mudu_sys::env_var::temp_dir().join(format!("epoch-image-{}", mudu_sys::random::uuid_v4()))
}
async fn log(dir: &std::path::Path) -> Arc<DurableStreamLog> {
    Arc::new(
        DurableStreamLog::open(dir.to_path_buf(), identity(), epoch_stream_descriptor())
            .await
            .unwrap(),
    )
}
fn op(stable: u64, current: u64) -> Vec<u8> {
    encode(&EpochOp::Advance {
        stable: EPOCH_XID_BASE + stable,
        current: EPOCH_XID_BASE + current,
        physical_millis: 123,
    })
    .unwrap()
}

#[test]
fn replicas_export_identical_committed_state_despite_local_watermarks() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let root = dir();
        let log = log(&root.join("log")).await;
        let a = ClusterEpochState::new();
        let b = ClusterEpochState::new();
        b.current.store(EPOCH_XID_BASE + 100, Ordering::Release);
        b.stable.store(EPOCH_XID_BASE + 50, Ordering::Release);
        let first = Arc::new(
            EpochStreamMachine::open(&log, &root.join("a"), a)
                .await
                .unwrap(),
        );
        let second = Arc::new(
            EpochStreamMachine::open(&log, &root.join("b"), b.clone())
                .await
                .unwrap(),
        );
        for (id, payload) in [(1, op(2, 4)), (2, op(1, 3)), (3, op(5, 6))] {
            let (prefix, _) = log.append(id, payload, vec![]).await.unwrap();
            let (_, record) = log.record(prefix.end_lsn).await.unwrap();
            first.apply_once(id, record.clone()).await.unwrap();
            second.apply_once(id, record).await.unwrap();
        }
        let prefix = first.applied_frontier().await.unwrap();
        let bytes = LoggedStreamAdapter::new(log.clone(), first.clone())
            .snapshot_at(prefix.as_ref())
            .await
            .unwrap();
        assert_eq!(
            bytes,
            LoggedStreamAdapter::new(log.clone(), second)
                .snapshot_at(prefix.as_ref())
                .await
                .unwrap()
        );
        assert_eq!(b.stable.load(Ordering::Acquire), EPOCH_XID_BASE + 50);
        assert_eq!(b.current.load(Ordering::Acquire), EPOCH_XID_BASE + 100);
        assert!(first.snapshot_at(None).await.is_err());
        let snapshot = EpochSnapshot::decode(&bytes, identity(), prefix.as_ref()).unwrap();
        for phase in 0..2 {
            let target = root.join(format!("target-{phase}"));
            let target_log = log_at(&target).await;
            if phase == 1 {
                let image = crate::log_stream::LoggedStreamSnapshot::decode(
                    &bytes,
                    identity(),
                    &epoch_stream_descriptor(),
                    prefix.as_ref(),
                )
                .unwrap();
                target_log.restore_snapshot(image.log()).await.unwrap();
            }
            snapshot
                .restore(&target_log, &target.join("apply"))
                .await
                .unwrap();
            snapshot
                .restore(&target_log, &target.join("apply"))
                .await
                .unwrap();
            let recovered = ClusterEpochState::new();
            EpochStreamMachine::open(&target_log, &target.join("apply"), recovered.clone())
                .await
                .unwrap();
            assert_eq!(recovered.stable.load(Ordering::Acquire), EPOCH_XID_BASE + 5);
            assert_eq!(
                recovered.current.load(Ordering::Acquire),
                EPOCH_XID_BASE + 6
            );
            assert_eq!(
                target_log.append(3, op(5, 6), vec![]).await.unwrap().0,
                prefix.clone().unwrap()
            );
        }
    })
    .unwrap();
}
async fn log_at(root: &std::path::Path) -> Arc<DurableStreamLog> {
    log(&root.join("log")).await
}

#[test]
fn epoch_images_reject_forged_state_and_conflicting_targets() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let root = dir();
        let source = log(&root.join("log")).await;
        let machine = Arc::new(
            EpochStreamMachine::open(&source, &root.join("apply"), ClusterEpochState::new())
                .await
                .unwrap(),
        );
        let (prefix, _) = source.append(1, op(2, 3), vec![]).await.unwrap();
        machine
            .apply_once(1, source.record(0).await.unwrap().1)
            .await
            .unwrap();
        let business: EpochImage =
            decode(&machine.snapshot_at(Some(&prefix)).await.unwrap()).unwrap();
        for mutation in 0..5 {
            let mut forged = business.clone();
            match mutation {
                0 => forged.version += 1,
                // A checkpoint behind the committed history is a regression
                // (the sparse in-memory advance may only move it forward).
                1 => forged.checkpoint.stable -= 1,
                2 => forged.checkpoint.current -= 1,
                3 => forged.checkpoint.physical_millis -= 1,
                _ => forged.prefix.as_mut().unwrap().segment_digest[0] ^= 1,
            }
            let bytes = encode(&(
                1_u32,
                source.capture_snapshot(Some(&prefix)).await.unwrap(),
                encode(&forged).unwrap(),
            ))
            .unwrap();
            assert!(EpochSnapshot::decode(&bytes, identity(), Some(&prefix)).is_err());
        }
        let bytes = LoggedStreamAdapter::new(source.clone(), machine)
            .snapshot_at(Some(&prefix))
            .await
            .unwrap();
        assert!(EpochSnapshot::decode(&bytes, identity(), None).is_err());
        assert!(EpochSnapshot::decode(
            &bytes,
            GroupIdobject {
                generation: 3,
                ..identity()
            },
            Some(&prefix)
        )
        .is_err());
        let snapshot = EpochSnapshot::decode(&bytes, identity(), Some(&prefix)).unwrap();
        let target = log(&root.join("target-log")).await;
        let descriptor = epoch_stream_descriptor();
        let (frontier, _) = FrontierStore::open(
            &root.join("target-apply"),
            "epoch.frontier",
            descriptor.stream_id,
            1,
        )
        .await
        .unwrap();
        let mut forged = business.checkpoint;
        forged.stable -= 1;
        frontier
            .store(&prefix, &encode(&forged).unwrap())
            .await
            .unwrap();
        assert!(snapshot
            .restore(&target, &root.join("target-apply"))
            .await
            .is_err());
        assert!(target.record(0).await.is_err());
        assert_eq!(
            frontier
                .load(mudu_sys::fs::tokio_fs().as_ref())
                .await
                .unwrap()
                .unwrap()
                .1,
            encode(&forged).unwrap()
        );
        let state = ClusterEpochState::new();
        assert!(
            EpochStreamMachine::open(&target, &root.join("target-apply"), state.clone())
                .await
                .is_err()
        );
        target
            .restore_snapshot(&source.capture_snapshot(Some(&prefix)).await.unwrap())
            .await
            .unwrap();
        assert!(
            EpochStreamMachine::open(&target, &root.join("target-apply"), state.clone())
                .await
                .is_err()
        );
        assert_eq!(state.stable.load(Ordering::Acquire), EPOCH_XID_BASE);
    })
    .unwrap();
}

#[test]
fn empty_epoch_snapshot_and_apply_ordering() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let root = dir();
        let log = log(&root.join("log")).await;
        let machine = Arc::new(
            EpochStreamMachine::open(&log, &root.join("apply"), ClusterEpochState::new())
                .await
                .unwrap(),
        );
        let bytes = LoggedStreamAdapter::new(log.clone(), machine.clone())
            .snapshot_at(None)
            .await
            .unwrap();
        let image = EpochSnapshot::decode(&bytes, identity(), None).unwrap();
        image.restore(&log, &root.join("apply")).await.unwrap();
        let (prefix, _) = log.append(1, op(2, 3), vec![]).await.unwrap();
        let (_, record) = log.record(0).await.unwrap();
        let mut wrong = record.clone();
        wrong.progress.end_lsn = 1;
        assert!(machine.apply_once(1, wrong).await.is_err());
        let mut wrong = record.clone();
        wrong.identity.generation += 1;
        assert!(machine.apply_once(1, wrong).await.is_err());
        let mut wrong = record.clone();
        wrong.payload = encode(&EpochOp::Advance {
            stable: u64::MAX,
            current: u64::MAX,
            physical_millis: 0,
        })
        .unwrap();
        assert!(machine.apply_once(1, wrong).await.is_err());
        assert!(machine.applied_frontier().await.unwrap().is_none());
        machine.apply_once(1, record.clone()).await.unwrap();
        machine.apply_once(1, record.clone()).await.unwrap();
        let mut wrong = record;
        wrong.progress.segment_digest[0] ^= 1;
        assert!(machine.apply_once(1, wrong).await.is_err());
        assert!(image.restore(&log, &root.join("apply")).await.is_err());
        assert_eq!(machine.applied_frontier().await.unwrap(), Some(prefix));
    })
    .unwrap();
}

#[test]
fn sparse_advance_persists_and_recovers_ahead_of_committed_history() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let root = dir();
        let source = log(&root.join("log")).await;
        let state = ClusterEpochState::new();
        let machine = Arc::new(
            EpochStreamMachine::open(&source, &root.join("apply"), state.clone())
                .await
                .unwrap(),
        );
        // Commit a reserve (the range is already large enough, but the record
        // pins the committed history the in-memory advance will cover).
        let (prefix, _) = source
            .append(
                1,
                encode(&EpochOp::Reserve {
                    reserved_upper: EPOCH_XID_BASE + 2_000_000,
                })
                .unwrap(),
                vec![],
            )
            .await
            .unwrap();
        machine
            .apply_once(1, source.record(0).await.unwrap().1)
            .await
            .unwrap();
        // Advance in-memory within the reserved range.
        machine
            .advance_in_memory(EPOCH_XID_BASE, EPOCH_XID_BASE + 5, 42)
            .await
            .unwrap();
        assert_eq!(state.stable.load(Ordering::Acquire), EPOCH_XID_BASE);
        assert_eq!(state.current.load(Ordering::Acquire), EPOCH_XID_BASE + 5);
        // Persist the in-memory checkpoint and recover it on reopen.
        machine.checkpoint().await.unwrap();
        drop(machine);
        let reopened =
            EpochStreamMachine::open(&source, &root.join("apply"), ClusterEpochState::new())
                .await
                .unwrap();
        assert_eq!(
            reopened.checkpoint_state().unwrap().current,
            EPOCH_XID_BASE + 5
        );
        let _ = prefix;
    })
    .unwrap();
}
