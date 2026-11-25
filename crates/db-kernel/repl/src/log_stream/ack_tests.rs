//! Quorum, namespace and session fencing tests for generic streams.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::tests::{adapters, descriptor, identity, progress, start_node, wait_leader};
use super::*;
use mudu_raft::stream::{MembershipVersion, RegistryOp};
use mudu_raft::{ConfNodeValue, ConfValue, NodeInfo, ReplEntry};

fn conf(voters: &[u64], learners: &[u64], version: u64) -> ConfNodeValue {
    let nodes = voters
        .iter()
        .map(|id| NodeInfo {
            node_id: *id,
            can_vote: true,
        })
        .chain(learners.iter().map(|id| NodeInfo {
            node_id: *id,
            can_vote: false,
        }))
        .collect();
    ConfNodeValue::new(ConfValue::new(1, 4, nodes), 1, version, 0)
}
fn round(committed: &ConfNodeValue, incoming: &ConfNodeValue) -> AckRound {
    AckRound {
        identity: identity(),
        registry_version: 1,
        leader_term: 9,
        membership_version: MembershipVersion::from_conf(committed, incoming),
        codec_version: 1,
        digest_version: 1,
        progress: progress(1, 7),
    }
}
fn ack(round: &AckRound, node: u64, session: u128) -> DurableAck {
    DurableAck {
        node_id: node,
        session_id: session,
        round: round.clone(),
    }
}

#[test]
fn joint_stream_quorum_excludes_learners_and_revokes_old_sessions() {
    let old = conf(&[1, 2, 3], &[6], 1);
    let incoming = conf(&[3, 4, 5], &[6], 2);
    let round = round(&old, &incoming);
    let mut tracker = AckTracker::new(round.clone(), old, incoming, 1).unwrap();
    tracker.record_local_durable().unwrap();
    for node in [2, 3, 4, 6] {
        tracker.set_peer_session(node, node as u128).unwrap();
    }
    for node in [2, 3, 6] {
        assert!(tracker.record_peer(&ack(&round, node, node as u128)));
    }
    assert!(!tracker.quorum_met());
    assert!(tracker.record_peer(&ack(&round, 4, 4)));
    let proof = tracker.proof().unwrap();
    tracker.set_peer_session(3, 33).unwrap();
    assert!(!proof.guard.is_valid());
    assert!(!tracker.quorum_met());
    assert!(!tracker.record_peer(&ack(&round, 3, 3)));
    assert!(tracker.record_peer(&ack(&round, 3, 33)));
    assert!(tracker.proof().unwrap().guard.is_valid());
}

#[test]
fn every_ack_scope_component_and_exact_digest_must_match() {
    let old = conf(&[1, 2, 3], &[], 1);
    let round = round(&old, &old);
    let mut tracker = AckTracker::new(round.clone(), old.clone(), old, 1).unwrap();
    tracker.record_local_durable().unwrap();
    tracker.set_peer_session(2, 22).unwrap();
    let correct = ack(&round, 2, 22);
    let mutations: Vec<fn(&mut DurableAck)> = vec![
        |a| a.round.identity.cluster_id += 1,
        |a| a.round.identity.group_id.0 += 1,
        |a| a.round.identity.generation += 1,
        |a| a.round.registry_version += 1,
        |a| a.round.leader_term += 1,
        |a| a.round.progress.stream_id.0 += 1,
        |a| a.round.progress.stream_generation += 1,
        |a| a.round.progress.end_lsn += 1,
        |a| a.round.progress.segment_digest[0] ^= 1,
        |a| a.round.codec_version += 1,
        |a| a.round.digest_version += 1,
        |a| a.round.membership_version.incoming.version += 1,
        |a| a.node_id = 3,
        |a| a.session_id += 1,
    ];
    for mutate in mutations {
        let mut stale = correct.clone();
        mutate(&mut stale);
        assert!(!tracker.record_peer(&stale));
        assert!(!tracker.quorum_met());
    }
    assert!(tracker.record_peer(&correct));
    assert!(tracker.quorum_met());
}

#[test]
fn local_durability_is_explicit_and_duplicate_peer_acks_do_not_add_votes() {
    let membership = conf(&[1, 2, 3], &[], 1);
    let round = round(&membership, &membership);
    let mut tracker = AckTracker::new(round.clone(), membership.clone(), membership, 1).unwrap();
    tracker.set_peer_session(2, 2).unwrap();
    for _ in 0..3 {
        assert!(tracker.record_peer(&ack(&round, 2, 2)));
    }
    assert!(tracker.proof().is_err());
    tracker.record_local_durable().unwrap();
    let proof = tracker.proof().unwrap();
    drop(tracker);
    assert!(!proof.guard.is_valid());
}

#[test]
fn stream_payload_is_durable_outside_raft_before_progress_commit() {
    let path =
        mudu_sys::env_var::temp_dir().join(format!("stream-proof-{}", mudu_sys::random::uuid_v4()));
    let raft = start_node(&path);
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        wait_leader(&raft).await;
        let service = ReplicatedStreamRegistry::open(
            identity(),
            raft.clone(),
            path.join("registry.ckpt"),
            adapters(),
        )
        .await
        .unwrap();
        service
            .change(1, RegistryOp::Register(descriptor(1)))
            .await
            .unwrap();
        let (committed, incoming) = raft.conf_async().await.unwrap();
        let bytes = vec![0x6a; 16 * 1024];
        let digest = crate::seg_hash::seg_hash_chain(
            &crate::seg_hash::ZERO_SEG_HASH,
            std::slice::from_ref(&bytes),
        );
        let mut target = progress(1, 0);
        target.segment_digest = digest;
        let mut round = round(&committed, &incoming);
        round.leader_term = raft.term();
        round.progress = target.clone();
        let mut tracker = AckTracker::new(round, committed, incoming, 1).unwrap();
        assert!(tracker.proof().is_err());
        let fs = mudu_sys::fs::tokio_fs();
        let file = fs
            .open(
                &path.join("payload.segment"),
                mudu_sys::contract::file_options::FileOptions::read_write_create(),
            )
            .await
            .unwrap();
        file.write_all_at(0, &bytes).await.unwrap();
        file.fsync().await.unwrap();
        fs.sync_dir(&path).await.unwrap();
        tracker.record_local_durable().unwrap();
        service
            .commit_progress(vec![tracker.proof().unwrap()])
            .await
            .unwrap();
        assert_eq!(
            service.refresh().await.unwrap().committed()[&target.stream_id],
            target
        );
        let committed = raft.read_committed_async(0, 64).await.unwrap();
        let entries: Vec<_> = committed
            .entries
            .into_iter()
            .filter_map(|(_, entry)| matches!(entry, ReplEntry::GroupCommit(_)).then_some(entry))
            .collect();
        assert_eq!(entries.len(), 1);
        assert!(rmp_serde::to_vec(&entries[0]).unwrap().len() < 1024);
        let revoked = tracker.proof().unwrap();
        drop(tracker);
        assert!(service.commit_progress(vec![revoked]).await.is_err());
        assert_eq!(
            service.refresh().await.unwrap().committed()[&target.stream_id],
            target
        );
        raft.stop_async().await.unwrap();
    });
    drop(raft);
    mudu_sys::fs::sync::remove_dir_all(path).unwrap();
}
