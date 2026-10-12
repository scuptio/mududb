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
    let (_committed, new) = raft.conf().map_err(|error| ReplMembershipError {
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
    let waiter = raft
        .update_conf(conf_value, nid_vote, nid_log)
        .map_err(|err| membership_error(err, http_port))?;
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

/// Reads the current committed conf into the endpoint's member view.
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
