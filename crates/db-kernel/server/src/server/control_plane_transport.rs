//! The production stream transport of the control plane: leader→member
//! frame push and member→member heal fetch over the cluster channel
//! (the wire types live in `catalog::contract::control_plane`).
//!
//! Session discipline: the leader side keeps one session per peer for the
//! service lifetime and revokes every session guard on drop, fencing
//! proposals still queued for Raft admission. The receiver side keys
//! sessions by `(sender, session id)`; when a sender reappears with a new
//! session id (a restarted control service), the old session's guard is
//! revoked so its in-flight receives cannot complete.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use mudu_utils::channel::Channel;

use ::catalog::contract::control_plane::{
    StreamFetchRequest, StreamProgressRequest, StreamPushRequest, StreamRoundWire,
};
use ::catalog::contract::derive_oid::derive_node_endpoint;
use ::mudu_raft::stream::{GroupId, GroupIdobject, MembershipVersion, StreamId, StreamProgress};
use ::mudu_raft::{AdmissionGuard, ConfVersion};
pub(crate) use ::repl::log_stream::ReceiverSessions;
use ::repl::log_stream::{AckRound, DurableAck, StreamFetch, StreamSession, StreamTransport};

/// Timeout of one stream push/fetch RPC; the replication group wraps calls
/// in its own slightly larger deadline.
const STREAM_RPC_TIMEOUT: Duration = Duration::from_secs(4);

/// Converts a mechanism round identity into its wire form.
pub(crate) fn round_to_wire(round: &AckRound) -> StreamRoundWire {
    StreamRoundWire {
        cluster_id: round.identity.cluster_id,
        group_id: round.identity.group_id.0,
        group_generation: round.identity.generation,
        registry_version: round.registry_version,
        leader_term: round.leader_term,
        committed_conf: conf_to_wire(&round.membership_version.committed),
        incoming_conf: conf_to_wire(&round.membership_version.incoming),
        codec_version: round.codec_version,
        digest_version: round.digest_version,
        stream_id: round.progress.stream_id.0,
        stream_generation: round.progress.stream_generation,
        end_lsn: round.progress.end_lsn,
        segment_digest: round.progress.segment_digest,
    }
}

/// Converts a wire round identity back into the mechanism form.
pub(crate) fn round_from_wire(wire: &StreamRoundWire) -> AckRound {
    AckRound {
        identity: GroupIdobject {
            cluster_id: wire.cluster_id,
            group_id: GroupId(wire.group_id),
            generation: wire.group_generation,
        },
        registry_version: wire.registry_version,
        leader_term: wire.leader_term,
        membership_version: MembershipVersion {
            committed: conf_from_wire(wire.committed_conf),
            incoming: conf_from_wire(wire.incoming_conf),
        },
        codec_version: wire.codec_version,
        digest_version: wire.digest_version,
        progress: StreamProgress {
            stream_id: StreamId(wire.stream_id),
            stream_generation: wire.stream_generation,
            end_lsn: wire.end_lsn,
            segment_digest: wire.segment_digest,
        },
    }
}

fn conf_to_wire(conf: &ConfVersion) -> (u64, u64, u64) {
    (conf.term, conf.version, conf.index)
}

fn conf_from_wire(wire: (u64, u64, u64)) -> ConfVersion {
    ConfVersion {
        term: wire.0,
        version: wire.1,
        index: wire.2,
    }
}

/// Converts a mechanism durable ack into its wire form.
pub(crate) fn ack_to_wire(ack: &DurableAck) -> ::catalog::contract::control_plane::StreamAckWire {
    ::catalog::contract::control_plane::StreamAckWire {
        node_id: ack.node_id,
        session_id: ack.session_id,
        round: round_to_wire(&ack.round),
    }
}

/// The leader-side stream transport over the cluster channel.
pub(crate) struct ClusterStreamTransport {
    node_id: u64,
    channel: Channel,
    /// One session per peer for the lifetime of this transport.
    sessions: SMutex<HashMap<u64, StreamSession>>,
}

impl ClusterStreamTransport {
    /// Creates the transport bound to this node's endpoint channel.
    pub(crate) fn new(node_id: u64, channel: Channel) -> Self {
        Self {
            node_id,
            channel,
            sessions: SMutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl StreamTransport for ClusterStreamTransport {
    async fn session(&self, peer: u64) -> RS<StreamSession> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "stream sessions lock poisoned"))?;
        if let Some(session) = sessions.get(&peer) {
            if session.valid.is_valid() {
                return Ok(session.clone());
            }
        }
        let session = StreamSession {
            id: mudu_sys::random::uuid_v4().as_u128(),
            valid: AdmissionGuard::new(),
        };
        sessions.insert(peer, session.clone());
        Ok(session)
    }

    async fn push(
        &self,
        peer: u64,
        session: &StreamSession,
        round: &AckRound,
        frame: Vec<u8>,
    ) -> RS<Option<DurableAck>> {
        let request = StreamPushRequest {
            session_id: session.id,
            from_node: self.node_id,
            round: round_to_wire(round),
            frame,
        };
        let response = self
            .channel
            .endpoint::<StreamPushRequest>(derive_node_endpoint(peer))
            .ask(request, STREAM_RPC_TIMEOUT)
            .await?;
        Ok(response.ack.map(|ack| DurableAck {
            node_id: ack.node_id,
            session_id: ack.session_id,
            round: round_from_wire(&ack.round),
        }))
    }

    async fn fetch(&self, peer: u64, session: &StreamSession, request: StreamFetch) -> RS<Vec<u8>> {
        let wire = StreamFetchRequest {
            session_id: session.id,
            from_node: self.node_id,
            cluster_id: request.identity.cluster_id,
            group_id: request.identity.group_id.0,
            group_generation: request.identity.generation,
            stream_id: request.stream_id.0,
            stream_generation: request.stream_generation,
            lsn: request.lsn,
        };
        let response = self
            .channel
            .endpoint::<StreamFetchRequest>(derive_node_endpoint(peer))
            .ask(wire, STREAM_RPC_TIMEOUT)
            .await?;
        Ok(response.frame)
    }

    async fn progress_at(
        &self,
        peer: u64,
        session: &StreamSession,
        stream_id: StreamId,
        lsn: u64,
    ) -> RS<Option<StreamProgress>> {
        let wire = StreamProgressRequest {
            session_id: session.id,
            from_node: self.node_id,
            cluster_id: 0,
            group_id: 0,
            group_generation: 0,
            stream_id: stream_id.0,
            lsn,
        };
        let response = self
            .channel
            .endpoint::<StreamProgressRequest>(derive_node_endpoint(peer))
            .ask(wire, STREAM_RPC_TIMEOUT)
            .await?;
        Ok(response.progress.map(|wire| StreamProgress {
            stream_id: StreamId(wire.stream_id),
            stream_generation: wire.stream_generation,
            end_lsn: wire.end_lsn,
            segment_digest: wire.segment_digest,
        }))
    }
}

impl Drop for ClusterStreamTransport {
    fn drop(&mut self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            for (_, session) in sessions.drain() {
                session.valid.invalidate();
            }
        }
    }
}
