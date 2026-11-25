//! The client face of the cluster's catalog control stream (used by DDL
//! executors, served by the unified control plane): proposes to a node endpoint and
//! follows leader redirects.
//!
//! Leader discovery (N-02): a process-wide cache remembers each home node's
//! last known control leader with the term that produced the hint, so the
//! next `apply` starts at the leader instead of bouncing off the local node;
//! an older-term hint never overwrites a newer one.

use async_trait::async_trait;
use catalog::contract::derive_oid::derive_node_endpoint;
use catalog::contract::meta_op::{MetaGroupApi, MetaGroupRpcRequest, MetaGroupRpcResponse, MetaOp};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use mudu_utils::channel::current_channel;
use std::collections::HashMap;
use std::time::Duration;
use tracing::debug;

/// Timeout of one control-plane propose round (commit + local apply).
const META_PROPOSE_TIMEOUT: Duration = Duration::from_secs(15);
/// Backoff before re-asking after a stale leader hint.
const META_LEADER_RETRY_BACKOFF: Duration = Duration::from_millis(100);
/// Upper bound of leader rediscovery retries of one `apply` call.
const META_APPLY_MAX_ATTEMPTS: u32 = 20;

/// Process-wide control-leader hints: home node id → (leader, term).
/// Tests run several nodes in one process; keying by the client's home node
/// keeps each node's view distinct.
fn leader_hints() -> &'static SMutex<HashMap<u64, (u64, u64)>> {
    static HINTS: std::sync::OnceLock<SMutex<HashMap<u64, (u64, u64)>>> =
        std::sync::OnceLock::new();
    HINTS.get_or_init(|| SMutex::new(HashMap::new()))
}

/// Remembers a leader hint; an older term never overwrites a newer one.
fn note_leader(home: u64, leader: u64, term: u64) {
    if let Ok(mut hints) = leader_hints().lock() {
        let entry = hints.entry(home).or_insert((leader, term));
        if term >= entry.1 {
            *entry = (leader, term);
        }
    }
}

/// The last known leader of the home node's control group, when any.
fn hinted_leader(home: u64) -> Option<u64> {
    leader_hints().lock().ok()?.get(&home).map(|hint| hint.0)
}

/// Proposes meta operations to the cluster's control plane over the channel:
/// the first ask targets the cached leader when known (else this node's
/// endpoint); `NotLeader` responses redirect to the control leader directly.
pub struct ChannelMetaGroup {
    node_id: u64,
    /// Explicit channel binding (the management plane runs outside any
    /// worker's channel context); `None` resolves `current_channel()`.
    channel: Option<mudu_utils::channel::Channel>,
}

impl ChannelMetaGroup {
    /// Creates the client face; `node_id` is this node's cluster id.
    pub fn new(node_id: u64) -> Self {
        Self {
            node_id,
            channel: None,
        }
    }

    /// Creates the client face bound to an explicit channel (management
    /// plane callers without a current channel context).
    pub fn with_channel(node_id: u64, channel: mudu_utils::channel::Channel) -> Self {
        Self {
            node_id,
            channel: Some(channel),
        }
    }

    /// The bound channel or the current task's channel.
    fn channel(&self) -> RS<mudu_utils::channel::Channel> {
        match &self.channel {
            Some(channel) => Ok(channel.clone()),
            None => current_channel(),
        }
    }
}

#[async_trait]
impl MetaGroupApi for ChannelMetaGroup {
    async fn apply(&self, op: MetaOp) -> RS<()> {
        let channel = self.channel()?;
        let mut node = hinted_leader(self.node_id).unwrap_or(self.node_id);
        let mut attempts = 0;
        // One stable idempotency key for the whole operation: retries and
        // leader redirects reuse it, so the control plane's stream log dedups
        // them instead of double-applying a committed op.
        let request_id = mudu_sys::random::uuid_v4().as_u128();
        loop {
            attempts += 1;
            if attempts > META_APPLY_MAX_ATTEMPTS {
                return Err(mudu_error!(
                    ErrorCode::TimedOut,
                    "control plane apply did not find a serving leader in time"
                ));
            }
            let response = channel
                .endpoint::<MetaGroupRpcRequest>(derive_node_endpoint(node))
                .ask(
                    MetaGroupRpcRequest::Propose {
                        request_id,
                        op: Box::new(op.clone()),
                    },
                    META_PROPOSE_TIMEOUT,
                )
                .await;
            match response {
                Ok(MetaGroupRpcResponse::Applied) => {
                    note_leader(self.node_id, node, 0);
                    return Ok(());
                }
                Ok(MetaGroupRpcResponse::NotLeader { leader, term }) => {
                    if let Some(leader) = leader {
                        note_leader(self.node_id, leader, term);
                        node = leader;
                    }
                    mudu_sys::sleep(META_LEADER_RETRY_BACKOFF).await?;
                }
                Ok(MetaGroupRpcResponse::Err(err)) => {
                    return Err(mudu_error!(ErrorCode::Internal, err));
                }
                Err(err) => {
                    debug!(node, "control plane propose ask failed: {}", err);
                    mudu_sys::sleep(META_LEADER_RETRY_BACKOFF).await?;
                }
            }
        }
    }

    fn is_running(&self) -> bool {
        true
    }
}
