//! Runtime lifecycle regression tests.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{InMemoryNetwork, RaftCfg, RaftHandle, ReplEntry};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn cancelled_startup_does_not_block_the_shared_owner() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = RaftCfg::new(1, vec![1], 5, 4, dir.path().join("cancelled"));
    cfg.manual_ticks = true;
    let network = InMemoryNetwork::new();
    // The ready result will be dropped on the owner after recovery.
    drop(RaftHandle::start_waiter(cfg, network.transport(1)).unwrap());
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        let mut cfg = RaftCfg::new(2, vec![2], 5, 4, dir.path().join("next"));
        cfg.manual_ticks = true;
        let node = mudu_sys::timeout(
            Duration::from_secs(10),
            RaftHandle::start_async(cfg, network.transport(2)),
        )
        .await
        .unwrap()
        .unwrap();
        for _ in 0..6 {
            node.inject_tick().unwrap();
        }
        let mut status = node.watch_status_async().unwrap();
        mudu_sys::timeout(Duration::from_secs(10), async {
            loop {
                if status.borrow_and_update().role == crate::RaftRole::Leader {
                    break;
                }
                status.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        let result = node
            .propose(ReplEntry::Barrier { term: node.term() })
            .unwrap()
            .wait()
            .await
            .unwrap();
        assert_eq!(result.0, 1);
        node.stop_async().await.unwrap();
        node.stop_async().await.unwrap();
        assert!(node.propose(ReplEntry::Barrier { term: 1 }).is_err());
    });
}

#[test]
fn stale_payload_membership_proof_is_rejected_before_proposal() {
    let dir = tempfile::tempdir().unwrap();
    let node = RaftHandle::start(
        RaftCfg::new(1, vec![1], 1, 4, dir.path().to_path_buf()),
        InMemoryNetwork::new().transport(1),
    )
    .unwrap();
    let mut status = node.watch_status_async().unwrap();
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        mudu_sys::timeout(Duration::from_secs(10), async {
            loop {
                if status.borrow_and_update().role == crate::RaftRole::Leader {
                    break;
                }
                status.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        let (committed, incoming) = node.conf_async().await.unwrap();
        let valid = crate::stream::MembershipVersion::from_conf(&committed, &incoming);
        let mut stale = valid.clone();
        stale.incoming.version += 1;
        let result = node
            .propose_under_membership(ReplEntry::Barrier { term: node.term() }, stale)
            .unwrap()
            .wait()
            .await
            .unwrap_err();
        assert_eq!(result.ec(), mudu::error::ErrorCode::ResourceBusy);
        assert_eq!(node.commit_index(), 0);
        node.propose_under_membership(ReplEntry::Barrier { term: node.term() }, valid)
            .unwrap()
            .wait()
            .await
            .unwrap();
        assert_eq!(node.commit_index(), 1);
        node.stop_async().await.unwrap();
    });
}

#[test]
fn synchronous_waits_reject_async_executor_threads() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = RaftCfg::new(1, vec![1], 5, 4, dir.path().to_path_buf());
    cfg.manual_ticks = true;
    let node = RaftHandle::start(cfg, InMemoryNetwork::new().transport(1)).unwrap();
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        assert_eq!(
            node.conf().unwrap_err().ec(),
            mudu::error::ErrorCode::InvalidState
        );
        let waiter = node.propose(ReplEntry::Barrier { term: 1 }).unwrap();
        assert!(waiter
            .recv()
            .unwrap_err()
            .to_string()
            .contains("asynchronous Raft API"));
        // Rejecting a synchronous wait must not consume its completion.
        assert!(waiter.wait().await.is_err());
        assert_eq!(
            node.stop().unwrap_err().ec(),
            mudu::error::ErrorCode::InvalidState
        );
        assert!(node.conf_async().await.is_ok());
        node.stop_async().await.unwrap();
    });
}

struct OwnerProbe {
    from: u64,
    samples: mudu_sys::ChannelSender<(u64, std::thread::ThreadId)>,
}

impl crate::RaftTransport for OwnerProbe {
    fn send(&self, _to: u64, _msg: crate::RaftMsg) -> mudu::common::result::RS<()> {
        self.samples.send((self.from, std::thread::current().id()))
    }
}

#[test]
fn independent_groups_share_one_protocol_owner_thread() {
    let dir = tempfile::tempdir().unwrap();
    let (samples, received) = mudu_sys::unbounded_channel();
    let mut nodes = Vec::new();
    for id in [1, 3] {
        let mut cfg = RaftCfg::new(id, vec![id, id + 1], 5, 4, dir.path().join(id.to_string()));
        cfg.manual_ticks = true;
        let node = RaftHandle::start(
            cfg,
            Arc::new(OwnerProbe {
                from: id,
                samples: samples.clone(),
            }),
        )
        .unwrap();
        for _ in 0..6 {
            node.inject_tick().unwrap();
        }
        nodes.push(node);
    }
    let mut groups = std::collections::HashSet::new();
    let mut owners = std::collections::HashSet::new();
    while groups.len() < 2 {
        let (group, owner) = received
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        groups.insert(group);
        owners.insert(owner);
    }
    assert_eq!(owners.len(), 1);
    for node in nodes {
        node.stop().unwrap();
    }
}

#[test]
fn membership_fence_rejects_changed_prefix_term_configuration_and_session() {
    let dir = tempfile::tempdir().unwrap();
    let node = RaftHandle::start(
        RaftCfg::new(1, vec![1], 1, 4, dir.path().to_path_buf()),
        InMemoryNetwork::new().transport(1),
    )
    .unwrap();
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        let mut status = node.watch_status_async().unwrap();
        mudu_sys::timeout(Duration::from_secs(10), async {
            while status.borrow_and_update().role != crate::RaftRole::Leader {
                status.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        let (old, new) = node.conf_async().await.unwrap();
        let guard = crate::AdmissionGuard::new();
        let proof = crate::MembershipFence {
            term: node.term(),
            membership: crate::stream::MembershipVersion::from_conf(&old, &new),
            committed_index: node.commit_index(),
            guards: vec![guard.clone()],
        };
        let mut stale_term = proof.clone();
        stale_term.term += 1;
        assert_membership_rejected(&node, stale_term).await;
        let mut stale_conf = proof.clone();
        stale_conf.membership.incoming.version += 1;
        assert_membership_rejected(&node, stale_conf).await;
        guard.invalidate();
        assert_membership_rejected(&node, proof.clone()).await;
        let mut valid = proof.clone();
        valid.guards = vec![crate::AdmissionGuard::new()];
        node.propose(ReplEntry::Barrier { term: node.term() })
            .unwrap()
            .wait()
            .await
            .unwrap();
        assert_membership_rejected(&node, valid.clone()).await;
        valid.committed_index = node.commit_index();
        let (conf, vote, log) = new.derive_add_learner(2).unwrap();
        node.update_conf_fenced_async(conf, vote, log, Some(valid))
            .await
            .unwrap();
        node.stop_async().await.unwrap();
    });
}

async fn assert_membership_rejected(node: &RaftHandle, proof: crate::MembershipFence) {
    let (_, new) = node.conf_async().await.unwrap();
    let (conf, vote, log) = new.derive_add_learner(2).unwrap();
    let result = node
        .update_conf_fenced_async(conf, vote, log, Some(proof))
        .await;
    let error = mudu::error::MuduError::from(
        result
            .map(|_| ())
            .expect_err("stale membership fence accepted"),
    );
    assert_eq!(error.ec(), mudu::error::ErrorCode::ResourceBusy);
    assert_eq!(node.conf_async().await.unwrap().1, new);
}
