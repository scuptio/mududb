//! Protocol frame types: message kinds, frame header, and frame.

use super::format;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys_contract::perf::TraceContext;
use serde::{Deserialize, Serialize};

/// Kind of message carried by a protocol frame.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageType {
    /// Protocol handshake.
    Handshake = 1,
    /// Authentication.
    Auth = 2,
    /// SQL query request.
    Query = 3,
    /// SQL execute request.
    Execute = 4,
    /// SQL batch request.
    Batch = 5,
    /// Generic response.
    Response = 6,
    /// Error response.
    Error = 7,
    /// Key-value get request.
    Get = 8,
    /// Key-value put request.
    Put = 9,
    /// Key-value range scan request.
    RangeScan = 10,
    /// Procedure invocation request.
    ProcedureInvoke = 11,
    /// Session creation request.
    SessionCreate = 12,
    /// Session close request.
    SessionClose = 13,
    /// Replication subscribe request.
    ReplSubscribe = 14,
    /// Replication WAL batch.
    ReplBatch = 15,
    /// Replication acknowledgement.
    ReplAck = 16,
    /// Replication heal fetch request.
    ReplHealFetch = 17,
    /// Replication heal fetch response.
    ReplHealResp = 18,
    /// Replication snapshot offer.
    ReplSnapshotOffer = 29,
    /// Replication snapshot file fetch request.
    ReplSnapshotFetch = 30,
    /// Replication snapshot file fetch response.
    ReplSnapshotResp = 31,
    /// Replication subscription acceptance.
    ReplSubscribeAccept = 32,
    /// Raft pre-vote request.
    RaftPreVoteReq = 19,
    /// Raft pre-vote response.
    RaftPreVoteResp = 20,
    /// Raft vote request.
    RaftVoteReq = 21,
    /// Raft vote response.
    RaftVoteResp = 22,
    /// Raft append-entries request.
    RaftAppendReq = 23,
    /// Raft append-entries response.
    RaftAppendResp = 24,
    /// Raft apply-snapshot request.
    RaftApplyReq = 25,
    /// Raft apply-snapshot response.
    RaftApplyResp = 26,
    /// Raft configuration-sync request.
    RaftUpdateConfReq = 27,
    /// Raft configuration-sync response.
    RaftUpdateConfResp = 28,
}

impl From<MessageType> for u32 {
    fn from(value: MessageType) -> Self {
        value as u32
    }
}

impl TryFrom<u32> for MessageType {
    type Error = mudu::error::MuduError;

    fn try_from(value: u32) -> RS<Self> {
        match value {
            1 => Ok(MessageType::Handshake),
            2 => Ok(MessageType::Auth),
            3 => Ok(MessageType::Query),
            4 => Ok(MessageType::Execute),
            5 => Ok(MessageType::Batch),
            6 => Ok(MessageType::Response),
            7 => Ok(MessageType::Error),
            8 => Ok(MessageType::Get),
            9 => Ok(MessageType::Put),
            10 => Ok(MessageType::RangeScan),
            11 => Ok(MessageType::ProcedureInvoke),
            12 => Ok(MessageType::SessionCreate),
            13 => Ok(MessageType::SessionClose),
            14 => Ok(MessageType::ReplSubscribe),
            15 => Ok(MessageType::ReplBatch),
            16 => Ok(MessageType::ReplAck),
            17 => Ok(MessageType::ReplHealFetch),
            18 => Ok(MessageType::ReplHealResp),
            29 => Ok(MessageType::ReplSnapshotOffer),
            30 => Ok(MessageType::ReplSnapshotFetch),
            31 => Ok(MessageType::ReplSnapshotResp),
            32 => Ok(MessageType::ReplSubscribeAccept),
            19 => Ok(MessageType::RaftPreVoteReq),
            20 => Ok(MessageType::RaftPreVoteResp),
            21 => Ok(MessageType::RaftVoteReq),
            22 => Ok(MessageType::RaftVoteResp),
            23 => Ok(MessageType::RaftAppendReq),
            24 => Ok(MessageType::RaftAppendResp),
            25 => Ok(MessageType::RaftApplyReq),
            26 => Ok(MessageType::RaftApplyResp),
            27 => Ok(MessageType::RaftUpdateConfReq),
            28 => Ok(MessageType::RaftUpdateConfResp),
            _ => Err(mudu_error!(
                ErrorCode::Parse,
                format!("unknown message type {}", value)
            )),
        }
    }
}

/// Fixed-size header of a protocol frame.
#[derive(Debug, Clone)]
pub struct FrameHeader {
    pub(crate) magic: u32,
    pub(crate) version: u32,
    pub(crate) message_type: MessageType,
    pub(crate) flags: u64,
    pub(crate) request_id: u64,
    pub(crate) trace_context: TraceContext,
    pub(crate) payload_len: u32,
}

/// A protocol frame: header plus payload.
#[derive(Debug, Clone)]
pub struct Frame {
    pub(crate) header: FrameHeader,
    pub(crate) payload: Vec<u8>,
}

impl Frame {
    /// Creates a frame of `message_type` with an empty trace context.
    pub fn new(message_type: MessageType, request_id: u64, payload: Vec<u8>) -> Self {
        Self::new_with_trace(message_type, request_id, TraceContext::empty(), payload)
    }

    /// Creates a frame of `message_type` carrying `trace_context`.
    pub fn new_with_trace(
        message_type: MessageType,
        request_id: u64,
        trace_context: TraceContext,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            header: FrameHeader::new_with_trace(
                message_type,
                request_id,
                trace_context,
                payload.len() as u32,
            ),
            payload,
        }
    }

    /// Creates a frame from a header and payload, checking their lengths agree.
    pub fn from_parts(header: FrameHeader, payload: Vec<u8>) -> RS<Self> {
        if header.payload_len() as usize != payload.len() {
            return Err(mudu_error!(
                ErrorCode::Parse,
                format!(
                    "frame payload length mismatch: header {}, actual {}",
                    header.payload_len(),
                    payload.len()
                )
            ));
        }
        Ok(Self { header, payload })
    }

    /// Encodes the frame into its wire byte form.
    pub fn encode(&self) -> Vec<u8> {
        format::encode_latest(self)
    }

    /// Decodes a frame from its wire byte form.
    pub fn decode(buf: &[u8]) -> RS<Self> {
        format::decode(buf)
    }

    /// Returns the frame header.
    pub fn header(&self) -> &FrameHeader {
        &self.header
    }

    /// Returns the frame payload.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Consumes the frame and returns the payload.
    pub fn into_payload(self) -> Vec<u8> {
        self.payload
    }
}

impl FrameHeader {
    /// Creates a header of `message_type` with an empty trace context.
    pub fn new(message_type: MessageType, request_id: u64, payload_len: u32) -> Self {
        Self::new_with_trace(message_type, request_id, TraceContext::empty(), payload_len)
    }

    /// Creates a header of `message_type` carrying `trace_context`.
    pub fn new_with_trace(
        message_type: MessageType,
        request_id: u64,
        trace_context: TraceContext,
        payload_len: u32,
    ) -> Self {
        Self {
            magic: format::latest::MAGIC,
            version: format::latest::FRAME_VERSION,
            message_type,
            flags: if trace_context.sampled {
                format::latest::FLAG_SAMPLED
            } else {
                0
            },
            request_id,
            trace_context,
            payload_len,
        }
    }

    /// Decodes a header from its wire byte form.
    pub fn decode_header_bytes(buf: &[u8]) -> RS<Self> {
        format::decode_header_bytes(buf)
    }

    /// Returns the protocol magic number.
    pub fn magic(&self) -> u32 {
        self.magic
    }

    /// Returns the protocol frame format version.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Returns the message type of the frame.
    pub fn message_type(&self) -> MessageType {
        self.message_type
    }

    /// Returns the frame flags.
    pub fn flags(&self) -> u64 {
        self.flags
    }

    /// Returns whether trace sampling is enabled for this frame.
    pub fn sampled(&self) -> bool {
        self.trace_context.sampled
    }

    /// Returns the request id correlating request and response.
    pub fn request_id(&self) -> u64 {
        self.request_id
    }

    /// Returns the trace context of the frame.
    pub fn trace_context(&self) -> TraceContext {
        self.trace_context
    }

    /// Returns the payload length in bytes.
    pub fn payload_len(&self) -> u32 {
        self.payload_len
    }
}
