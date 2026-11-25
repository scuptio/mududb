use super::*;
use crate::data_plane::DecisionSnapshot;
use crate::log_stream::{frontier::decode, FrontierStore};
use mudu::common::crc::crc64;

async fn image(f: &Fixture) -> Vec<u8> {
    let prefix = f.machine.applied_frontier().await.unwrap();
    encode(&(
        1_u32,
        f.log.capture_snapshot(prefix.as_ref()).await.unwrap(),
        f.machine.snapshot_at(prefix.as_ref()).await.unwrap(),
    ))
    .unwrap()
}

#[test]
fn offline_restore_replays_results_and_resumes_interrupted_phases() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        for (i, (cmd, _)) in lifecycle_commands(&f.coordinator).iter().enumerate() {
            f.append(i as u128, cmd, true).await;
        }
        let pending = CoordinatorRef {
            tx_id: 99,
            ..f.coordinator.clone()
        };
        f.append(100, &lifecycle_commands(&pending)[1].0, true)
            .await;
        let bytes = image(&f).await;
        let prefix = f.machine.applied_frontier().await.unwrap();
        let snapshot = DecisionSnapshot::decode(&bytes, f.log.identity(), prefix.as_ref()).unwrap();
        for phase in 0..3 {
            let root = f.dir.join(format!("restored-{phase}"));
            let log = DurableStreamLog::open(
                root.join("log"),
                f.log.identity(),
                tx_decision_descriptor(),
            )
            .await
            .unwrap();
            if phase > 0 {
                log.restore_snapshot(snapshot_log(&bytes, &f, prefix.as_ref()).log())
                    .await
                    .unwrap();
            }
            if phase > 1 {
                let (frontier, _) = FrontierStore::open(
                    &root.join("apply"),
                    "tx_decision.frontier",
                    super::super::TX_DECISION_STREAM,
                    1,
                )
                .await
                .unwrap();
                frontier.store(prefix.as_ref().unwrap(), &[]).await.unwrap();
                let recovered = TxDecisionMachine::open(&log, &root.join("apply"))
                    .await
                    .unwrap();
                assert_eq!(recovered.pending().unwrap(), f.machine.pending().unwrap());
            }
            snapshot.restore(&log, &root.join("apply")).await.unwrap();
            snapshot.restore(&log, &root.join("apply")).await.unwrap();
            let restored = TxDecisionMachine::open(&log, &root.join("apply"))
                .await
                .unwrap();
            assert_eq!(
                restored.state(&f.coordinator).unwrap(),
                f.machine.state(&f.coordinator).unwrap()
            );
            assert_eq!(restored.pending().unwrap(), f.machine.pending().unwrap());
            for lsn in 0..=prefix.as_ref().unwrap().end_lsn {
                assert_eq!(
                    restored.result(lsn).unwrap(),
                    f.machine.result(lsn).unwrap()
                );
            }
            let begin = encode(&lifecycle_commands(&pending)[1].0).unwrap();
            assert_eq!(
                log.append(100, begin, vec![]).await.unwrap().0,
                prefix.clone().unwrap()
            );
        }
    })
    .unwrap();
}

fn snapshot_log(
    bytes: &[u8],
    f: &Fixture,
    prefix: Option<&mudu_raft::stream::StreamProgress>,
) -> crate::log_stream::LoggedStreamSnapshot {
    crate::log_stream::LoggedStreamSnapshot::decode(
        bytes,
        f.log.identity(),
        &tx_decision_descriptor(),
        prefix,
    )
    .unwrap()
}

#[test]
fn valid_checksum_cannot_hide_missing_transaction_or_false_result() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        f.begin().await;
        let prefix = f.machine.applied_frontier().await.unwrap();
        let business = f.machine.snapshot_at(prefix.as_ref()).await.unwrap();
        let (_, payload): (u64, Vec<u8>) = decode(&business).unwrap();
        let (version, identity, view): (u32, GroupIdobject, super::super::view::View) =
            decode(&payload).unwrap();
        for mutation in 0..3 {
            let mut altered = view.clone();
            match mutation {
                0 => altered.transactions.clear(),
                1 => {
                    altered.results.insert(
                        0,
                        Err(catalog::contract::tx_decision::TxDecisionError::MissingBegin),
                    );
                }
                _ => altered.applied.as_mut().unwrap().segment_digest[0] ^= 1,
            }
            let payload = encode(&(version, identity, altered)).unwrap();
            let business = encode(&(crc64(&payload), payload)).unwrap();
            let bytes = encode(&(
                1_u32,
                f.log.capture_snapshot(prefix.as_ref()).await.unwrap(),
                business,
            ))
            .unwrap();
            assert!(DecisionSnapshot::decode(&bytes, identity, prefix.as_ref()).is_err());
        }
        let bytes = image(&f).await;
        assert!(DecisionSnapshot::decode(&bytes, identity, None).is_err());
        let foreign = GroupIdobject {
            generation: identity.generation + 1,
            ..identity
        };
        assert!(DecisionSnapshot::decode(&bytes, foreign, prefix.as_ref()).is_err());
    })
    .unwrap();
}

#[test]
fn empty_restore_and_conflicting_target_fail_closed() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let f = Fixture::new().await;
        let empty = DecisionSnapshot::decode(&image(&f).await, f.log.identity(), None).unwrap();
        let root = f.dir.join("target");
        let log =
            DurableStreamLog::open(root.join("log"), f.log.identity(), tx_decision_descriptor())
                .await
                .unwrap();
        empty.restore(&log, &root.join("apply")).await.unwrap();
        let restored = TxDecisionMachine::open(&log, &root.join("apply"))
            .await
            .unwrap();
        assert!(restored.pending().unwrap().is_empty());
        drop(restored);
        f.begin().await;
        let prefix = f.machine.applied_frontier().await.unwrap();
        let full =
            DecisionSnapshot::decode(&image(&f).await, f.log.identity(), prefix.as_ref()).unwrap();
        let (frontier, _) = FrontierStore::open(
            &root.join("apply"),
            "tx_decision.frontier",
            super::super::TX_DECISION_STREAM,
            1,
        )
        .await
        .unwrap();
        let mut wrong = prefix.clone().unwrap();
        wrong.segment_digest[0] ^= 1;
        frontier.store(&wrong, &[]).await.unwrap();
        assert!(full.restore(&log, &root.join("apply")).await.is_err());
        assert!(log.record(0).await.is_err());
        assert_eq!(
            frontier
                .load(mudu_sys::fs::tokio_fs().as_ref())
                .await
                .unwrap()
                .unwrap()
                .0,
            wrong
        );
        let fs = mudu_sys::fs::tokio_fs();
        fs.remove_dir_all(&root.join("apply")).await.unwrap();
        fs.create_dir_all(&root.join("apply")).await.unwrap();
        let checkpoint = root.join("apply/tx_decision.snapshot");
        crate::durable_io::publish(fs.as_ref(), &checkpoint, b"conflicting checkpoint")
            .await
            .unwrap();
        assert!(full.restore(&log, &root.join("apply")).await.is_err());
        assert!(log.record(0).await.is_err());
        assert_eq!(
            fs.read_all(&checkpoint).await.unwrap(),
            b"conflicting checkpoint"
        );
        fs.remove_dir_all(&root.join("apply")).await.unwrap();
        full.restore(&log, &root.join("apply")).await.unwrap();
        assert!(empty.restore(&log, &root.join("apply")).await.is_err());
        assert!(TxDecisionMachine::open(&log, &root.join("apply"))
            .await
            .unwrap()
            .state(&f.coordinator)
            .unwrap()
            .is_some());
    })
    .unwrap();
}
