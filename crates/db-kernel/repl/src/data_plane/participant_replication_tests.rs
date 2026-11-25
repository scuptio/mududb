//! Prepare and decision travel through the same real data stream quorum.
use super::*;
use catalog::contract::tx_decision::{
    CoordinatorRef, TxDecision, TxDecisionClient, TxDecisionStruct as R, TxParticipant,
};
use wal::participant::{ParticipantDecision, ParticipantPrepare};

async fn append_op(node: &Node, op: TxOp) {
    let log = node.worker.worker_log().unwrap().unwrap();
    let batch = XLBatch::new(vec![XLEntry {
        xid: 99,
        ops: vec![op],
        commit_epoch: None,
    }]);
    let frames = log.serialize_entry(&batch).unwrap();
    let lsns = wal::log_frame::frame_lsns(&frames).unwrap();
    let first = lsns.first().unwrap().as_u64();
    log.enqueue_group_commit(frames.clone(), lsns, true)
        .await
        .unwrap();
    node.service
        .append_data_segment(0, first, DataSegment { version: 1, frames })
        .await
        .unwrap();
    // The real leader commit driver applies after the stream quorum returns.
    node.worker.replay_log_batch(batch).await.unwrap();
}

#[test]
fn follower_keeps_prepare_invisible_until_the_decision_commits() {
    let _guard = crate::control_plane::test_guard();
    let harness = mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let harness = Harness::start(&[1, 2]).await;
        let leader = harness.wait_leader().await;
        let node = harness.node(leader);
        let follower = harness.node(if leader == 1 { 2 } else { 1 });
        let tx_id = (1_u128 << 100) | 99;
        append_op(
            node,
            TxOp::Prepare(ParticipantPrepare {
                coordinator: node.service.coordinator_ref(tx_id),
                partition_id: 0,
                snapshot_epoch: EPOCH_XID_BASE,
                write_set: vec![XLWrite::Insert(XLInsert {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"prepared".to_vec(),
                    value: b"committed".to_vec(),
                })],
            }),
        )
        .await;
        mudu_sys::timeout(TIMEOUT, async {
            while !follower.worker.storage().has_participant_history().unwrap() {
                mudu_sys::task::async_::sleep(Duration::from_millis(5))
                    .await
                    .unwrap();
            }
        })
        .await
        .unwrap();
        assert_eq!(
            follower
                .worker
                .worker_x_contract()
                .worker_get_async(b"prepared")
                .await
                .unwrap(),
            None
        );
        assert!(
            crate::snapshot::take_worker_snapshot(&follower.worker, 0, 0, [0; 32], 0, 0)
                .await
                .is_err()
        );
        append_op(
            node,
            TxOp::ParticipantDecision(ParticipantDecision {
                coordinator: node.service.coordinator_ref(tx_id),
                partition_id: 0,
                decision: TxDecision::Committed {
                    commit_epoch: EPOCH_XID_BASE + 1,
                },
                commit_ts: Default::default(),
            }),
        )
        .await;
        wait_kv(&follower.worker, b"prepared", b"committed").await;
        harness
    })
    .unwrap();
    harness.stop();
}

struct LiveShare(Weak<DataPlaneService>);
impl catalog::repl_ctrl::RaftShareView for LiveShare {
    fn is_serving_leader(&self) -> bool {
        self.0
            .upgrade()
            .is_some_and(|service| service.is_serving_leader())
    }
    fn leader_hint(&self) -> Option<mudu_contract::protocol::LeaderHint> {
        None
    }
    fn leader_node_id(&self) -> Option<u64> {
        self.0
            .upgrade()
            .and_then(|service| service.raft().status().leader_id)
    }
    fn term_source(&self) -> Arc<std::sync::atomic::AtomicU64> {
        Arc::new(std::sync::atomic::AtomicU64::new(
            self.0
                .upgrade()
                .map_or(0, |service| service.raft().status().term),
        ))
    }
}

async fn run_recovery_with_driver(node: &Node) {
    node.worker
        .worker_x_contract()
        .set_raft_share(Arc::new(LiveShare(Arc::downgrade(&node.service))))
        .unwrap();
    let (handle, driver) = super::super::new_stream_commit_driver(
        0,
        node.worker.worker_log().unwrap().unwrap(),
        node.worker.storage(),
        node.service.clone(),
    )
    .unwrap();
    node.worker.set_commit_sequencer(Arc::new(handle)).unwrap();
    let (stop, stopped) = mudu_sys::sync::async_::stop_flag::stop_channel();
    let join = tokio::task::spawn_local(super::super::run_stream_commit_driver(driver, stopped));
    mudu_sys::timeout(
        TIMEOUT,
        node.worker
            .worker_x_contract()
            .recover_pending_cross_partition_records_async(),
    )
    .await
    .unwrap()
    .unwrap();
    stop.stop();
    join.await.unwrap().unwrap();
}

#[test]
fn new_leader_aborts_an_old_term_prepare_without_control_plane() {
    recover_after_failover(None);
}

#[test]
fn new_leader_finishes_a_durable_commit_without_control_plane() {
    recover_after_failover(Some(TxDecision::Committed {
        commit_epoch: EPOCH_XID_BASE + 1,
    }));
}

fn recover_after_failover(decision: Option<TxDecision>) {
    let _guard = crate::control_plane::test_guard();
    let harness = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let harness = Harness::start(&[1, 2, 3]).await;
        let old = harness.wait_leader().await;
        let node = harness.node(old);
        let client = node
            .worker
            .worker_x_contract()
            .tx_decision_client()
            .unwrap();
        let coordinator = client.coordinator_ref(888).unwrap();
        let _activity = client.track_coordinator(&coordinator).unwrap();
        let term = client.leader_term().await.unwrap();
        begin_prepare(node, client.as_ref(), &coordinator, term).await;
        mudu_sys::timeout(TIMEOUT, async {
            while harness
                .nodes
                .iter()
                .any(|node| !node.worker.storage().has_participant_history().unwrap())
            {
                mudu_sys::task::async_::sleep(Duration::from_millis(5))
                    .await
                    .unwrap();
            }
        })
        .await
        .unwrap();
        // Same-term recovery must not abort a live coordinator.
        run_recovery_with_driver(node).await;
        assert_eq!(
            client
                .transaction_state(&coordinator)
                .await
                .unwrap()
                .unwrap()
                .decision,
            None
        );
        if let Some(decision) = decision {
            client
                .submit(
                    8881,
                    R::Decide {
                        coordinator: coordinator.clone(),
                        decision,
                    },
                )
                .await
                .unwrap();
        }
        harness._raft_net.isolate(old).unwrap();
        harness._stream_net.blocked.lock().unwrap().insert(old);
        let successor = wait_successor(&harness, old).await;
        let node = harness.node(successor);
        assert!(node.service.raft().status().term > term);
        run_recovery_with_driver(node).await;
        let state = node
            .service
            .transaction_state(&coordinator)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            state.decision,
            Some(decision.unwrap_or(TxDecision::Aborted))
        );
        assert!(state.complete);
        assert_eq!(
            node.worker
                .worker_x_contract()
                .worker_get_async(b"in-doubt")
                .await
                .unwrap(),
            decision.map(|_| b"invisible".to_vec())
        );
        harness
    })
    .unwrap();
    harness.stop();
}

async fn wait_successor(harness: &Harness, old: u64) -> u64 {
    mudu_sys::timeout(TIMEOUT, async {
        loop {
            if let Some(node) = harness.nodes.iter().find(|node| {
                node.service.raft().node_id() != old && node.service.is_serving_leader()
            }) {
                return node.service.raft().node_id();
            }
            mudu_sys::task::async_::sleep(Duration::from_millis(5))
                .await
                .unwrap();
        }
    })
    .await
    .unwrap()
}

async fn begin_prepare(
    node: &Node,
    client: &dyn TxDecisionClient,
    coordinator: &CoordinatorRef,
    term: u64,
) {
    client
        .submit(
            8880,
            R::Begin {
                coordinator: coordinator.clone(),
                owner_term: term,
                participants: vec![TxParticipant {
                    group_id: 1,
                    group_generation: 1,
                    worker_index: 0,
                    partition_id: 0,
                }],
                snapshot_epoch: EPOCH_XID_BASE,
            },
        )
        .await
        .unwrap();
    append_op(
        node,
        TxOp::Prepare(ParticipantPrepare {
            coordinator: coordinator.clone(),
            partition_id: 0,
            snapshot_epoch: EPOCH_XID_BASE,
            write_set: vec![XLWrite::Insert(XLInsert {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key: b"in-doubt".to_vec(),
                value: b"invisible".to_vec(),
            })],
        }),
    )
    .await;
}

#[test]
fn cancelled_coordinator_recovers_without_a_new_term() {
    let _guard = crate::control_plane::test_guard();
    let harness = mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let harness = Harness::start(&[1]).await;
        let leader = harness.wait_leader().await;
        let node = harness.node(leader);
        let client = node
            .worker
            .worker_x_contract()
            .tx_decision_client()
            .unwrap();
        let coordinator = client.coordinator_ref(888).unwrap();
        let activity = client.track_coordinator(&coordinator).unwrap();
        assert!(client.track_coordinator(&coordinator).is_err());
        let term = client.leader_term().await.unwrap();
        begin_prepare(node, client.as_ref(), &coordinator, term).await;
        drop(activity);
        assert!(!client.coordinator_active(&coordinator).unwrap());
        run_recovery_with_driver(node).await;
        let state = client
            .transaction_state(&coordinator)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(node.service.raft().status().term, term);
        assert_eq!(state.decision, Some(TxDecision::Aborted));
        assert!(state.complete);
        harness
    })
    .unwrap();
    harness.stop();
}
