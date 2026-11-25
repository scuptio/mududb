use super::membership_gate::protocol_only_change;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::RaftHandle;
use std::sync::Arc;
use std::time::Duration;

/// Membership actions of the replication admin endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplMembershipAction {
    /// Add a node as a voter (replicates and votes).
    AddVoter,
    /// Add a node as a learner (replicates, never votes); the safe
    /// first step of joining a node.
    AddLearner,
    /// Remove a member.
    Remove,
    /// Promote a learner to voter after it has caught up.
    PromoteLearner,
}

/// HTTP redirect target of a not-the-leader membership rejection: the
/// current leader's node id and its management HTTP endpoint (symmetric
/// deployment: every node serves HTTP on the same port).
#[derive(Debug, Clone)]
pub struct ReplLeaderHttpHint {
    /// Leader's cluster node id.
    pub node_id: u64,
    /// Leader's host name or address.
    pub host: String,
    /// Leader's management HTTP port.
    pub http_port: u16,
}

/// Error of a membership request: the raft error plus an optional HTTP
/// redirect hint for not-the-leader failures.
#[derive(Debug)]
pub struct ReplMembershipError {
    /// The underlying error.
    pub error: mudu::error::MuduError,
    /// Redirect target when this node is not the leader and the leader is
    /// known with a registered address.
    pub leader_hint: Option<ReplLeaderHttpHint>,
}

/// One member of the resulting committed membership view.
#[derive(Debug, Clone)]
pub struct ReplMembershipMember {
    /// Cluster node id.
    pub node_id: u64,
    /// Whether the member votes (false = learner).
    pub can_vote: bool,
    /// Registered replication host, when known.
    pub host: Option<String>,
    /// Registered replication port, when known.
    pub port: Option<u16>,
}

/// The committed membership view returned by a successful change.
#[derive(Debug, Clone)]
pub struct ReplMembershipView {
    /// Members of the committed conf.
    pub members: Vec<ReplMembershipMember>,
    /// Raft term of the committed conf version.
    pub conf_term: u64,
    /// Committed conf version.
    pub conf_version: u64,
}

/// How long a membership change may take to commit before it is reported
/// as timed out (the change itself may still complete in the background).
const MEMBERSHIP_COMMIT_TIMEOUT: Duration = Duration::from_secs(30);

/// Applies one membership change against this server's raft group:
/// derives the joint triple, begins the re-configuration, waits for the
/// joint commit and aligns the kernel-side address book. Blocking; the
/// HTTP layer bridges it off the actix runtime.
pub fn raft_membership_update(
    server_node_id: u64,
    action: ReplMembershipAction,
    member_node_id: u64,
    member_host: Option<String>,
    member_port: Option<u16>,
    http_port: u16,
) -> Result<ReplMembershipView, ReplMembershipError> {
    let raft = crate::raft_handle_for_node(server_node_id).ok_or_else(|| ReplMembershipError {
        error: mudu_error!(
            ErrorCode::InvalidState,
            "this server is not running a raft group (membership requires [replication.raft])"
        ),
        leader_hint: None,
    })?;
    let (committed, new) = raft.conf().map_err(|error| ReplMembershipError {
        error,
        leader_hint: None,
    })?;
    let triple = match action {
        ReplMembershipAction::AddVoter => new.derive_add_voter(member_node_id),
        ReplMembershipAction::AddLearner => new.derive_add_learner(member_node_id),
        ReplMembershipAction::Remove => new.derive_remove_node(member_node_id),
        ReplMembershipAction::PromoteLearner => new.derive_promote_voter(member_node_id),
    };
    let (conf_value, nid_vote, nid_log) = triple.map_err(|error| ReplMembershipError {
        error,
        leader_hint: None,
    })?;
    // The new member's replication address must be known before the joint
    // commit: the conf-sync reconcile reads it to spawn the peer writer.
    if matches!(
        action,
        ReplMembershipAction::AddVoter | ReplMembershipAction::AddLearner
    ) {
        let (Some(host), Some(port)) = (member_host, member_port) else {
            return Err(ReplMembershipError {
                error: mudu_error!(
                    ErrorCode::InvalidArgument,
                    "adding a member requires its replication host and port"
                ),
                leader_hint: None,
            });
        };
        if let Err(error) = crate::repl_member_addr_register(member_node_id, host, port) {
            return Err(ReplMembershipError {
                error,
                leader_hint: None,
            });
        }
    }
    let expected = mudu_raft::stream::MembershipVersion::from_conf(&committed, &new);
    let waiter =
        begin_membership_change(&raft, expected, conf_value, nid_vote, nid_log, http_port)?;
    let deadline = *mudu_sys::time::instant_now() + MEMBERSHIP_COMMIT_TIMEOUT;
    loop {
        match waiter.recv_timeout(Duration::from_secs(1)) {
            Ok(Some(Ok(version))) => {
                let members = conf_members(&raft);
                return Ok(ReplMembershipView {
                    members,
                    conf_term: version.term,
                    conf_version: version.version,
                });
            }
            Ok(Some(Err(err))) => {
                return Err(membership_error(err, http_port));
            }
            Ok(None) => {
                if *mudu_sys::time::instant_now() >= deadline {
                    return Err(ReplMembershipError {
                        error: mudu_error!(
                            ErrorCode::TimedOut,
                            "raft re-configuration did not commit in time; it may still complete in the background"
                        ),
                        leader_hint: None,
                    });
                }
            }
            Err(err) => {
                return Err(ReplMembershipError {
                    error: err,
                    leader_hint: None,
                });
            }
        }
    }
}

/// Routes stream-bearing groups through the durable catch-up gate.
fn begin_membership_change(
    raft: &RaftHandle,
    expected: mudu_raft::stream::MembershipVersion,
    conf_value: mudu_raft::ConfValue,
    nid_vote: Vec<u64>,
    nid_log: Vec<u64>,
    http_port: u16,
) -> Result<mudu_raft::UpdateConfWaiter, ReplMembershipError> {
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().map_err(|error| {
        ReplMembershipError {
            error,
            leader_hint: None,
        }
    })?;
    runtime
        .block_on(async {
            mudu_sys::timeout(MEMBERSHIP_COMMIT_TIMEOUT, async {
                if let Some(service) = crate::data_plane::data_plane_for_node(raft.node_id()) {
                    service
                        .group()
                        .update_membership(expected, conf_value, nid_vote, nid_log)
                        .await
                } else {
                    protocol_only_change(raft, expected, conf_value, nid_vote, nid_log).await
                }
            })
            .await
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::TimedOut,
                    "membership proof/admission timed out; an admitted change may still complete"
                )
            })?
        })
        .map_err(|error| ReplMembershipError {
            error,
            leader_hint: leader_hint(raft, http_port),
        })
}

/// Reads the current committed configuration into the endpoint member view.
fn conf_members(raft: &Arc<RaftHandle>) -> Vec<ReplMembershipMember> {
    let mut members = raft
        .conf()
        .map(|(committed, _)| {
            committed
                .value
                .node_peer
                .iter()
                .map(|peer| {
                    let addr = crate::repl_member_addr(peer.node_id);
                    ReplMembershipMember {
                        node_id: peer.node_id,
                        can_vote: peer.can_vote,
                        host: addr.as_ref().map(|(host, _)| host.clone()),
                        port: addr.map(|(_, port)| port),
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    members.sort_by_key(|member| member.node_id);
    members
}

fn leader_hint(raft: &RaftHandle, http_port: u16) -> Option<ReplLeaderHttpHint> {
    let node_id = raft.status().leader_id?;
    let (host, _) = crate::repl_member_addr(node_id)?;
    Some(ReplLeaderHttpHint {
        node_id,
        host,
        http_port,
    })
}

/// Maps an [`UpdateConfError`] to the endpoint error, resolving the leader
/// hint to an HTTP redirect target when the leader's address is known.
fn membership_error(err: mudu_raft::UpdateConfError, http_port: u16) -> ReplMembershipError {
    let leader_hint = err.leader_hint().and_then(|node_id| {
        crate::repl_member_addr(node_id).map(|(host, _)| ReplLeaderHttpHint {
            node_id,
            host,
            http_port,
        })
    });
    ReplMembershipError {
        error: err.into_mudu_error(),
        leader_hint,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use mudu_raft::core::role::RaftRole;
    use mudu_raft::{InMemoryNetwork, RaftCfg, ReplEntry};
    use mudu_sys::env_var::temp_dir;
    use mudu_sys::task::sync::spawn_thread;
    use std::path::PathBuf;

    /// A two-node in-memory raft cluster with manual ticks on both nodes:
    /// the same deterministic driving style as the vendored raft's own
    /// cluster tests. Node 1 boots as the single voter; node 2 joins later
    /// with the full peer list, the way a fresh joiner does.
    struct TestCluster {
        network: InMemoryNetwork,
        dir1: PathBuf,
        dir2: PathBuf,
        handle1: Arc<RaftHandle>,
        handle2: Option<Arc<RaftHandle>>,
    }

    const NODE1: u64 = 101;
    const NODE2: u64 = 102;
    const TICKS: u64 = 5;

    impl TestCluster {
        fn new() -> Self {
            let network = InMemoryNetwork::new();
            let dir1 = temp_dir().join(format!(
                "repl_membership_{}_{}",
                NODE1,
                mudu_utils::oid::gen_oid()
            ));
            let dir2 = temp_dir().join(format!(
                "repl_membership_{}_{}",
                NODE2,
                mudu_utils::oid::gen_oid()
            ));
            let mut cfg1 = RaftCfg::new(NODE1, vec![NODE1], 20, TICKS, dir1.clone());
            cfg1.manual_ticks = true;
            let handle1 = RaftHandle::start(cfg1, network.transport(NODE1)).unwrap();
            network.register(NODE1, handle1.input_sender()).unwrap();
            Self {
                network,
                dir1,
                dir2,
                handle1: Arc::new(handle1),
                handle2: None,
            }
        }

        fn tick(&self, n: u64) {
            for _ in 0..n {
                self.handle1.inject_tick().unwrap();
            }
        }

        fn elect_leader(&self) {
            // Tick processing is asynchronous: alternate tick bursts with
            // short sleeps so the event loop can work through the queue.
            for _ in 0..100 {
                self.tick(TICKS + 2);
                for _ in 0..50 {
                    if self.handle1.role() == RaftRole::Leader {
                        return;
                    }
                    mudu_sys::task::sync::sleep_blocking(Duration::from_millis(10));
                }
            }
            panic!(
                "node did not elect itself leader; status {:?}",
                self.handle1.status()
            );
        }

        fn commit_barrier(&self) {
            let waiter = self
                .handle1
                .propose(ReplEntry::Barrier {
                    term: self.handle1.term(),
                })
                .unwrap();
            for _ in 0..50 {
                self.tick(1);
                match waiter.recv_timeout(Duration::from_millis(100)) {
                    Ok(Some(result)) => {
                        result.unwrap();
                        return;
                    }
                    _ => continue,
                }
            }
            panic!("barrier did not commit in time");
        }

        fn start_joiner(&mut self) {
            let mut cfg2 = RaftCfg::new(NODE2, vec![NODE1, NODE2], 20, TICKS, self.dir2.clone());
            cfg2.manual_ticks = true;
            let handle2 = RaftHandle::start(cfg2, self.network.transport(NODE2)).unwrap();
            self.network
                .register(NODE2, handle2.input_sender())
                .unwrap();
            self.handle2 = Some(Arc::new(handle2));
        }

        /// Runs a blocking membership update on a helper thread while this
        /// thread ticks the leader until the update resolves.
        fn drive_update(
            &self,
            action: ReplMembershipAction,
            member: u64,
            host: Option<String>,
            port: Option<u16>,
        ) -> Result<ReplMembershipView, ReplMembershipError> {
            let worker = spawn_thread(move || {
                raft_membership_update(NODE1, action, member, host, port, 8080)
            })
            .unwrap();
            // Poll the helper thread by probing its result through a shared
            // slot: SJoinHandle has no try-join, so park on the join with a
            // ticking outer loop.
            let (done_tx, done_rx) = std::sync::mpsc::sync_channel(1);
            let waiter = spawn_thread(move || {
                let r = worker.join().unwrap();
                let _ = done_tx.send(r);
            })
            .unwrap();
            loop {
                self.tick(1);
                mudu_sys::task::sync::sleep_blocking(Duration::from_millis(5));
                match done_rx.try_recv() {
                    Ok(result) => {
                        waiter.join().unwrap();
                        return result;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        panic!("membership helper thread vanished")
                    }
                }
            }
        }
    }

    impl Drop for TestCluster {
        fn drop(&mut self) {
            let _ = crate::raft_handle_by_node_unregister(NODE1);
            let _ = crate::repl_member_addr_unregister(NODE2);
            if let Some(handle2) = &self.handle2 {
                let _ = handle2.stop();
            }
            let _ = self.handle1.stop();
            let _ = mudu_sys::fs::sync::remove_dir_all(&self.dir1);
            let _ = mudu_sys::fs::sync::remove_dir_all(&self.dir2);
        }
    }

    #[test]
    fn membership_update_requires_a_running_raft_group() {
        // No handle registered under this node id.
        let err = raft_membership_update(103, ReplMembershipAction::AddVoter, 2, None, None, 8080)
            .unwrap_err();
        assert_eq!(err.error.ec(), ErrorCode::InvalidState);
        assert!(err.leader_hint.is_none());
    }

    #[test]
    fn membership_update_learner_lifecycle() {
        let mut cluster = TestCluster::new();
        cluster.elect_leader();
        cluster.commit_barrier();
        crate::raft_handle_by_node_register(NODE1, cluster.handle1.clone()).unwrap();

        // A member add without its replication address is rejected before
        // any raft traffic.
        let err = cluster
            .drive_update(ReplMembershipAction::AddVoter, NODE2, None, None)
            .unwrap_err();
        assert_eq!(err.error.ec(), ErrorCode::InvalidArgument);

        // Removing a node that is not a member fails in the derive step.
        let err = cluster
            .drive_update(ReplMembershipAction::Remove, 99, None, None)
            .unwrap_err();
        assert!(err.leader_hint.is_none());

        cluster.start_joiner();

        // Add node 2 as a learner: the committed view lists it as
        // non-voting and its replication address is registered for the
        // conf-sync reconcile.
        let view = cluster
            .drive_update(
                ReplMembershipAction::AddLearner,
                NODE2,
                Some("127.0.0.1".to_string()),
                Some(19_102),
            )
            .unwrap();
        let learner = view
            .members
            .iter()
            .find(|member| member.node_id == NODE2)
            .expect("learner in view")
            .clone();
        assert!(!learner.can_vote);
        assert_eq!(learner.host.as_deref(), Some("127.0.0.1"));
        assert_eq!(learner.port, Some(19_102));
        assert_eq!(
            crate::repl_member_addr(NODE2),
            Some(("127.0.0.1".to_string(), 19_102))
        );

        // Promote the learner to voter, then remove it entirely.
        let promote = cluster.drive_update(ReplMembershipAction::PromoteLearner, NODE2, None, None);
        if let Err(e) = &promote {
            eprintln!("PROMOTE FAILED: {:?}", e.error);
            eprintln!("node1 status: {:?}", cluster.handle1.status());
            eprintln!("node1 conf: {:?}", cluster.handle1.conf());
            if let Some(h2) = &cluster.handle2 {
                eprintln!("node2 status: {:?}", h2.status());
            }
            panic!("promote failed");
        }
        let view = promote.unwrap();
        assert!(
            view.members
                .iter()
                .find(|member| member.node_id == NODE2)
                .expect("promoted member")
                .can_vote
        );
        let view = cluster
            .drive_update(ReplMembershipAction::Remove, NODE2, None, None)
            .unwrap();
        assert_eq!(view.members.len(), 1);
        assert_eq!(view.members[0].node_id, NODE1);
    }
}
