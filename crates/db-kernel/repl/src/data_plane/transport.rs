//! The production stream transport of a data group over the replication
//! listener (the MSFM channel): one lazily-established TCP connection per
//! group member, request/response framed with the shared protocol frames.
//!
//! Sessions are per-peer for the lifetime of this transport; dropping the
//! transport revokes every session guard, fencing proposals still queued for
//! Raft admission. A receiver that reappears with a new session id fences
//! its previous session's in-flight receives (server-side session table).

use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::net::AsyncTcpStream;
use mudu_sys::sync::async_::stop_flag::stop_channel;
use mudu_sys::sync::SMutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use catalog::contract::control_plane::{
    StreamFetchRequest, StreamFetchResponse, StreamProgressRequest, StreamProgressResponse,
    StreamPushRequest, StreamPushResponse,
};
use mudu_contract::protocol::{Frame, MessageType};
use mudu_raft::stream::{StreamId, StreamProgress};
use mudu_raft::AdmissionGuard;

use crate::log_stream::{AckRound, DurableAck, StreamFetch, StreamSession, StreamTransport};

/// One call's response timeout.
const STREAM_CALL_TIMEOUT: Duration = Duration::from_secs(4);

/// Per-peer lazily-established connection; one in-flight call at a time.
struct PeerConn {
    addr: String,
    slot: mudu_sys::tokio::sync::Mutex<Option<AsyncTcpStream>>,
}

/// The data group's stream transport over the replication listener.
pub struct ReplStreamTransport {
    node_id: u64,
    server_instance_id: mudu::channel::ServerInstanceId,
    peers: SMutex<HashMap<u64, Arc<PeerConn>>>,
    sessions: SMutex<HashMap<u64, StreamSession>>,
}

impl ReplStreamTransport {
    /// Creates the transport; peer addresses resolve through the kernel-side
    /// member address book (`repl_member_addr`).
    pub fn new(node_id: u64, server_instance_id: mudu::channel::ServerInstanceId) -> Self {
        Self {
            node_id,
            server_instance_id,
            peers: SMutex::new(HashMap::new()),
            sessions: SMutex::new(HashMap::new()),
        }
    }

    fn peer(&self, peer: u64) -> RS<Arc<PeerConn>> {
        let mut peers = self
            .peers
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "stream transport lock poisoned"))?;
        if let Some(conn) = peers.get(&peer) {
            return Ok(conn.clone());
        }
        let (host, port) = crate::repl_member_addr(peer).ok_or_else(|| {
            mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("no replication address for node {peer}")
            )
        })?;
        let conn = Arc::new(PeerConn {
            addr: format!("{host}:{port}"),
            slot: mudu_sys::tokio::sync::Mutex::new(None),
        });
        peers.insert(peer, conn.clone());
        Ok(conn)
    }

    /// One framed request/response call; a failed connection is dropped and
    /// re-established on the next call.
    async fn call(&self, peer: u64, msg_type: MessageType, payload: Vec<u8>) -> RS<Vec<u8>> {
        let conn = self.peer(peer)?;
        let mut slot = conn.slot.lock().await;
        if slot.is_none() {
            let stream = AsyncTcpStream::connect(conn.addr.clone())
                .await
                .map_err(|e| {
                    mudu_error!(
                        ErrorCode::Network,
                        format!("connect stream peer {} error", conn.addr),
                        e
                    )
                })?;
            stream
                .set_nodelay(true)
                .map_err(|e| mudu_error!(ErrorCode::Network, "set stream nodelay error", e))?;
            *slot = Some(stream);
        }
        let stream = slot.as_mut().expect("connection just established");
        let result = Self::call_on(stream, msg_type, payload).await;
        if result.is_err() {
            // Drop the broken connection; the next call reconnects.
            *slot = None;
        }
        result
    }

    async fn call_on(
        stream: &mut AsyncTcpStream,
        msg_type: MessageType,
        payload: Vec<u8>,
    ) -> RS<Vec<u8>> {
        use mudu_sys::tokio::io::AsyncWriteExt;
        let bytes = Frame::new(msg_type, 0, payload).encode();
        stream
            .write_all(&bytes)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, "write stream call error", e))?;
        let (_stop_tx, mut stop_rx) = stop_channel();
        let mut read_buf = Vec::with_capacity(8192);
        let mut chunk = vec![0u8; 8192];
        let frame = mudu_sys::timeout(
            STREAM_CALL_TIMEOUT,
            crate::read_next_frame(stream, &mut read_buf, &mut chunk, &mut stop_rx),
        )
        .await
        .ok_or_else(|| mudu_error!(ErrorCode::TimedOut, "stream call timed out"))??;
        let frame = frame.ok_or_else(|| {
            mudu_error!(ErrorCode::Network, "stream peer closed without a response")
        })?;
        if frame.header().message_type() == MessageType::Error {
            let error = mudu_contract::protocol::decode_error_response(&frame)?;
            return Err(mudu_error!(
                ErrorCode::from_u32(error.code()).unwrap_or(ErrorCode::Internal),
                format!("stream peer returned error: {}", error.message())
            ));
        }
        Ok(frame.into_payload())
    }
}

#[async_trait]
impl StreamTransport for ReplStreamTransport {
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
            round: crate::log_stream::wire::round_to_wire(round),
            frame,
        };
        let payload = rmp_serde::to_vec(&request)
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode stream push", e))?;
        let response = self
            .call(peer, MessageType::ReplStreamPush, payload)
            .await?;
        let response: StreamPushResponse = rmp_serde::from_slice(&response)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode stream push response", e))?;
        let ack = response.ack.map(|ack| DurableAck {
            node_id: ack.node_id,
            session_id: ack.session_id,
            round: crate::log_stream::wire::round_from_wire(&ack.round),
        });
        if let Some(ack) = &ack {
            // Leader-side observability: a durable push ack marks the peer as
            // a connected subscriber holding this data-stream watermark.
            if let Some(worker_index) = super::worker_index_of_stream(ack.round.progress.stream_id)
            {
                let _ = crate::register_repl_subscriber(
                    self.server_instance_id,
                    worker_index,
                    ack.node_id,
                );
                let _ = crate::record_repl_ack(
                    self.server_instance_id,
                    worker_index,
                    ack.node_id,
                    ack.round.progress.end_lsn,
                );
            }
        }
        Ok(ack)
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
        let payload = rmp_serde::to_vec(&wire)
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode fetch", e))?;
        let response = self
            .call(peer, MessageType::ReplStreamFetch, payload)
            .await?;
        let response: StreamFetchResponse = rmp_serde::from_slice(&response)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode fetch response", e))?;
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
        let payload = rmp_serde::to_vec(&wire)
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode progress query", e))?;
        let response = self
            .call(peer, MessageType::ReplStreamProgress, payload)
            .await?;
        let response: StreamProgressResponse = rmp_serde::from_slice(&response)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode progress response", e))?;
        Ok(response.progress.map(|wire| StreamProgress {
            stream_id: StreamId(wire.stream_id),
            stream_generation: wire.stream_generation,
            end_lsn: wire.end_lsn,
            segment_digest: wire.segment_digest,
        }))
    }
}

impl Drop for ReplStreamTransport {
    fn drop(&mut self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            for (_, session) in sessions.drain() {
                session.valid.invalidate();
            }
        }
    }
}
