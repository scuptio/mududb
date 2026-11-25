#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::completion::completion;
use crate::{RaftCfg, RaftStore, StoreCfg};
use mudu_sys::task::async_::build_current_thread_runtime;

#[test]
fn committed_notifications_observe_published_status() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = RaftCfg::new(1, vec![1], 5, 4, dir.path().to_path_buf());
    let runtime = build_current_thread_runtime().unwrap();
    let (store, _) = runtime
        .block_on(RaftStore::open_async(dir.path(), StoreCfg::default()))
        .unwrap();
    let (store, recovered) = runtime
        .block_on(crate::bootstrap_store_async(store, &cfg))
        .unwrap();
    let mut core = RaftCore::new(64, u64::MAX);
    core.recover(recovered);
    let version = core
        .raft_conf()
        .conf_node_value_committed()
        .term_version()
        .clone();
    let status = NodeStatus {
        node_id: 1,
        role: core.role(),
        term: core.term(),
        leader_id: core.leader_id(),
        commit_index: core.commit_index(),
        conf_version_committed: version.clone(),
        conf_version_new: version,
    };
    let shared = Arc::new(SRwLock::new(status));
    let (_input, input_rx) = crate::inbox::inbox();
    let mut node = RaftNode::new(
        core,
        input_rx,
        crate::InMemoryNetwork::new().transport(1),
        Box::new(store),
        shared.clone(),
        Duration::from_millis(5),
        true,
    )
    .unwrap();
    let mut fx = Effects::default();
    for _ in 0..6 {
        node.core.step_tick(&mut fx).unwrap();
    }
    runtime.block_on(node.flush_effects(fx)).unwrap();
    assert_eq!(node.core.role(), crate::RaftRole::Leader);

    let (first_tx, first_rx) = completion();
    let (second_tx, second_rx) = completion();
    let (watch_tx, mut watch_rx) = mudu_sys::tokio::sync::mpsc::channel(8);
    node.async_commits.push(watch_tx);
    let mut fx = Effects::default();
    for resp in [first_tx, second_tx] {
        let entry = ReplEntry::Barrier { term: 1 };
        match node.core.step_propose(entry, &mut fx) {
            ProposeOutcome::Accepted { index, term } => {
                node.pending.insert(index, (term, resp));
            }
            other => panic!("expected an accepted proposal, got {other:?}"),
        }
    }
    runtime.block_on(node.flush_effects(fx)).unwrap();
    let wait = Duration::from_secs(20);
    let proposal = first_rx.recv_timeout(wait).unwrap();
    assert_eq!(second_rx.recv().unwrap().unwrap(), (2, 1));
    let committed = runtime
        .block_on(mudu_sys::timeout(wait, watch_rx.recv()))
        .unwrap();
    let observed = shared.read().unwrap().clone();
    node.shutdown();

    assert_eq!(proposal.unwrap().unwrap(), (1, 1));
    assert_eq!(committed, Some((1, ReplEntry::Barrier { term: 1 })));
    assert_eq!(observed.commit_index, 2);
}

struct GatedStore {
    durable: mudu_sys::tokio::sync::oneshot::Receiver<RS<()>>,
}

impl crate::store::AsyncStore for GatedStore {
    fn apply_batch<'a>(
        &'a mut self,
        _ops: &'a [crate::core::message::WriteOp],
    ) -> crate::store::StoreFuture<'a> {
        Box::pin(async move {
            (&mut self.durable)
                .await
                .map_err(|_| mudu_error!(ErrorCode::Io, "injected IO cancellation"))?
        })
    }
    fn flush(&mut self) -> crate::store::StoreFuture<'_> {
        Box::pin(async { Ok(()) })
    }
}

fn test_node(store: Box<dyn crate::store::AsyncStore>) -> (RaftNode, crate::RaftSender) {
    let cfg = RaftCfg::new(1, vec![1], 5, 4, std::path::PathBuf::new());
    let conf = crate::ConfValue::new(
        1,
        cfg.election_timeout_ticks,
        vec![crate::NodeInfo {
            node_id: 1,
            can_vote: true,
        }],
    );
    let mut core = RaftCore::new(64, u64::MAX);
    core.recover(RaftCore::bootstrap_recovered(conf));
    let version = core
        .raft_conf()
        .conf_node_value_committed()
        .term_version()
        .clone();
    let shared = Arc::new(SRwLock::new(NodeStatus {
        node_id: 1,
        role: core.role(),
        term: core.term(),
        leader_id: core.leader_id(),
        commit_index: 0,
        conf_version_committed: version.clone(),
        conf_version_new: version,
    }));
    let (tx, rx) = crate::inbox::inbox();
    let node = RaftNode::new(
        core,
        rx,
        crate::InMemoryNetwork::new().transport(1),
        store,
        shared,
        Duration::from_millis(5),
        true,
    )
    .unwrap();
    (node, tx)
}

struct StructingTransport(Arc<std::sync::atomic::AtomicUsize>);

impl crate::RaftTransport for StructingTransport {
    fn send(&self, _to: u64, _msg: crate::RaftMsg) -> RS<()> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn slow_or_failed_disk_does_not_publish_protocol_effects() {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};
    for success in [true, false] {
        let runtime = build_current_thread_runtime().unwrap();
        runtime.block_on(async {
            let (durable, disk) = mudu_sys::tokio::sync::oneshot::channel();
            let (mut node, _tx) = test_node(Box::new(GatedStore { durable: disk }));
            let mut fx = Effects::default();
            for _ in 0..6 {
                node.core.step_tick(&mut fx).unwrap();
            }
            assert_eq!(node.core.role(), crate::RaftRole::Leader);
            let (resp, wait) = completion();
            match node
                .core
                .step_propose(ReplEntry::Barrier { term: 1 }, &mut fx)
            {
                ProposeOutcome::Accepted { index, term } => {
                    node.pending.insert(index, (term, resp));
                }
                other => panic!("unexpected outcome {other:?}"),
            }
            let sent = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            node.transport = Arc::new(StructingTransport(sent.clone()));
            fx.messages.push((
                2,
                crate::RaftMsg::ApplyResp(crate::core::message::ApplyResp {
                    term: 1,
                    match_index: 1,
                    id: "snapshot".into(),
                }),
            ));
            let shared = node.status_shared.clone();
            let mut flushing = Box::pin(node.flush_effects(fx));
            let mut context = Context::from_waker(Waker::noop());
            assert!(matches!(
                flushing.as_mut().poll(&mut context),
                Poll::Pending
            ));
            assert_eq!(shared.read().unwrap().commit_index, 0);
            assert_eq!(sent.load(std::sync::atomic::Ordering::SeqCst), 0);
            assert!(wait.recv_timeout(Duration::ZERO).unwrap().is_none());
            let result = if success {
                Ok(())
            } else {
                Err(mudu_error!(ErrorCode::Io, "injected fsync failure"))
            };
            durable.send(result).unwrap();
            if success {
                flushing.await.unwrap();
                assert_eq!(wait.recv().unwrap().unwrap(), (1, 1));
                assert_eq!(shared.read().unwrap().commit_index, 1);
                assert_eq!(sent.load(std::sync::atomic::Ordering::SeqCst), 1);
            } else {
                assert!(flushing.await.is_err());
                assert_eq!(shared.read().unwrap().commit_index, 0);
                assert_eq!(sent.load(std::sync::atomic::Ordering::SeqCst), 0);
                node.shutdown();
                assert!(wait.recv().unwrap().is_err());
            }
        });
    }
}

#[test]
fn stopping_during_io_waits_for_the_real_completion() {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};
    let runtime = build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        let (durable, disk) = mudu_sys::tokio::sync::oneshot::channel();
        let (node, tx) = test_node(Box::new(GatedStore { durable: disk }));
        for _ in 0..6 {
            tx.send(RaftIn::Tick).unwrap();
        }
        let mut running = Box::pin(node.run());
        let mut context = Context::from_waker(Waker::noop());
        // The first election persistence batch suspends the owner.
        assert!(matches!(running.as_mut().poll(&mut context), Poll::Pending));
        tx.send(RaftIn::Stop).unwrap();
        assert!(matches!(running.as_mut().poll(&mut context), Poll::Pending));
        durable.send(Ok(())).unwrap();
        running.await.unwrap();
    });
}

#[test]
fn certificate_revoked_after_enqueue_cannot_reach_the_core() {
    let runtime = build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        let (_durable, disk) = mudu_sys::tokio::sync::oneshot::channel();
        let (mut node, input) = test_node(Box::new(GatedStore { durable: disk }));
        let mut election = Effects::default();
        for _ in 0..6 {
            node.core.step_tick(&mut election).unwrap();
        }
        assert_eq!(node.core.role(), crate::RaftRole::Leader);
        let conf = crate::stream::MembershipVersion::from_conf(
            node.core.raft_conf().conf_node_value_committed(),
            node.core.raft_conf().conf_node_value_new(),
        );
        let guard = crate::AdmissionGuard::new();
        let (resp, wait) = completion();
        input
            .send(RaftIn::Propose {
                entry: ReplEntry::Barrier {
                    term: node.core.term(),
                },
                expected_conf: Some(conf),
                expected_term: Some(node.core.term()),
                guards: vec![guard.clone()],
                resp,
            })
            .unwrap();
        guard.invalidate();
        let event = node.in_rx.recv().await.unwrap();
        let mut fx = Effects::default();
        node.handle_input(Ok(Some(event)), &mut fx, &mut None, &mut None)
            .unwrap();
        assert_eq!(
            wait.recv().unwrap().unwrap_err().ec(),
            ErrorCode::ResourceBusy
        );
        assert_eq!(node.core.last_log_index(), 0);
        assert!(fx.writes.is_empty());
    });
}
