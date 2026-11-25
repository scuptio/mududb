//! Authenticated, bounded external-stream transport boundary.
use super::{AckRound, DurableAck};
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_raft::stream::{GroupIdobject, StreamId, StreamProgress};
use mudu_raft::AdmissionGuard;

/// Authenticated connection lifetime. The transport MUST invalidate the guard
/// when this session disconnects or is replaced, including during Raft admission.
#[derive(Clone)]
pub struct StreamSession {
    /// Nonzero token unique to this authenticated connection.
    pub id: u128,
    /// Revoked synchronously on connection replacement or disconnect.
    pub valid: AdmissionGuard,
}

/// Scope of a bounded missing-frame request.
#[derive(Clone, Debug)]
pub struct StreamFetch {
    /// Cluster, group and recovery generation.
    pub identity: GroupIdobject,
    /// Registered stream.
    pub stream_id: StreamId,
    /// Stream incarnation.
    pub stream_generation: u64,
    /// Exactly one requested LSN.
    pub lsn: u64,
}

/// External payload transport; Raft continues using its existing transport.
/// Implementations authenticate node/session identities, enforce frame byte
/// limits before allocation, bound queues and return Busy on saturation. They
/// must never synthesize durable ACKs before the receiver's fsync completes.
#[async_trait]
pub trait StreamTransport: Send + Sync {
    /// Resolves the currently authenticated session for one group member.
    async fn session(&self, peer: u64) -> RS<StreamSession>;
    /// Sends one frame. The peer acknowledges only when it holds the exact
    /// entire prefix in `round`; intermediate catch-up frames return None.
    async fn push(
        &self,
        peer: u64,
        session: &StreamSession,
        round: &AckRound,
        frame: Vec<u8>,
    ) -> RS<Option<DurableAck>>;
    /// Fetches one durable frame from a known group member during recovery.
    async fn fetch(&self, peer: u64, session: &StreamSession, request: StreamFetch) -> RS<Vec<u8>>;

    /// Queries a member's durable progress at one exact stream LSN; `None`
    /// when the member does not hold it. Used by promotion gates comparing a
    /// candidate's prefix against the committed vector (never trust a larger
    /// LSN as evidence for different content).
    async fn progress_at(
        &self,
        peer: u64,
        session: &StreamSession,
        stream_id: StreamId,
        lsn: u64,
    ) -> RS<Option<StreamProgress>>;

    /// Fetches one complete group snapshot bundle as a checksummed blob from a
    /// known member, for a RaftFollower whose stream prefix fell below the retained
    /// frontier. The default fails closed: only transports that ship snapshots
    /// implement it, and a transport that cannot must never fabricate one.
    async fn fetch_snapshot(
        &self,
        _peer: u64,
        _session: &StreamSession,
        _identity: GroupIdobject,
    ) -> RS<Vec<u8>> {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "stream transport does not ship group snapshots"
        ))
    }
}

/// The receiver-side session table: one guard per `(sender, session id)`;
/// a sender reappearing with a new session id revokes its previous sessions.
pub struct ReceiverSessions {
    sessions: mudu_sys::sync::SMutex<std::collections::HashMap<(u64, u128), AdmissionGuard>>,
}

impl Default for ReceiverSessions {
    fn default() -> Self {
        Self::new()
    }
}

impl ReceiverSessions {
    /// Creates an empty table.
    pub fn new() -> Self {
        Self {
            sessions: mudu_sys::sync::SMutex::new(std::collections::HashMap::new()),
        }
    }

    /// Resolves the receiver-side session of one incoming frame, revoking
    /// superseded sessions of the same sender.
    pub fn session_for(&self, from: u64, session_id: u128) -> RS<StreamSession> {
        let mut sessions = self.sessions.lock().map_err(|_| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Internal,
                "receiver sessions lock poisoned"
            )
        })?;
        if session_id == 0 {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::InvalidArgument,
                "stream session id must be nonzero"
            ));
        }
        let stale: Vec<(u64, u128)> = sessions
            .keys()
            .filter(|key| {
                let (sender, id) = **key;
                sender == from && id != session_id
            })
            .cloned()
            .collect();
        for key in stale {
            if let Some(guard) = sessions.remove(&key) {
                guard.invalidate();
            }
        }
        let guard = sessions
            .entry((from, session_id))
            .or_insert_with(AdmissionGuard::new)
            .clone();
        Ok(StreamSession {
            id: session_id,
            valid: guard,
        })
    }
}
