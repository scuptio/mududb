//! Client/server wire protocol frames and request/response types.
//!
//! This module defines the message types exchanged between a Mudu client and
//! server (SQL queries, key-value operations, procedure invocations, session
//! management), the replication and Raft frames, and the [`Frame`] /
//! [`FrameHeader`] codec used to carry them on the wire.

/// Frame wire encoding and decoding.
pub mod format;
/// Migration handlers for the wire protocol frame format.
pub mod migrate;
pub use format::latest::HEADER_LEN;

mod codec;
mod error;
mod frame;
mod message;
mod raft;
mod repl;

pub use codec::{
    decode_client_request, decode_error_response, decode_get_request, decode_get_response,
    decode_handshake_request, decode_handshake_response, decode_procedure_invoke_request,
    decode_procedure_invoke_response, decode_put_request, decode_put_response,
    decode_raft_append_request, decode_raft_append_response, decode_raft_apply_request,
    decode_raft_apply_response, decode_raft_pre_vote_request, decode_raft_pre_vote_response,
    decode_raft_update_conf_request, decode_raft_update_conf_response, decode_raft_vote_request,
    decode_raft_vote_response, decode_range_scan_request, decode_range_scan_response,
    decode_repl_ack_frame, decode_repl_batch_frame, decode_repl_heal_fetch_request,
    decode_repl_heal_response_frame, decode_repl_snapshot_fetch_request,
    decode_repl_snapshot_offer_frame, decode_repl_snapshot_resp_frame,
    decode_repl_subscribe_accept_frame, decode_repl_subscribe_request, decode_server_response,
    decode_session_close_request, decode_session_close_response, decode_session_create_request,
    decode_session_create_response, encode_batch_request, encode_client_request,
    encode_client_request_with_message_type, encode_client_request_with_message_type_and_trace,
    encode_error_response, encode_get_request, encode_get_response, encode_handshake_request,
    encode_handshake_response, encode_merror_response, encode_merror_response_with_leader_hint,
    encode_procedure_invoke_request, encode_procedure_invoke_request_with_trace,
    encode_procedure_invoke_response, encode_put_request, encode_put_response,
    encode_raft_append_request, encode_raft_append_response, encode_raft_apply_request,
    encode_raft_apply_response, encode_raft_pre_vote_request, encode_raft_pre_vote_response,
    encode_raft_update_conf_request, encode_raft_update_conf_response, encode_raft_vote_request,
    encode_raft_vote_response, encode_range_scan_request, encode_range_scan_response,
    encode_repl_ack_frame, encode_repl_batch_frame, encode_repl_heal_fetch_request,
    encode_repl_heal_response_frame, encode_repl_snapshot_fetch_request,
    encode_repl_snapshot_offer_frame, encode_repl_snapshot_resp_frame,
    encode_repl_subscribe_accept_frame, encode_repl_subscribe_request, encode_server_response,
    encode_session_close_request, encode_session_close_response, encode_session_create_request,
    encode_session_create_response,
};
pub use error::{ErrorResponse, LeaderHint};
pub use frame::{Frame, FrameHeader, MessageType};
pub use message::{
    ClientRequest, GetRequest, GetResponse, HandshakeRequest, HandshakeResponse, KeyValue,
    ProcedureInvokeRequest, ProcedureInvokeResponse, PutRequest, PutResponse, RangeScanRequest,
    RangeScanResponse, ServerPerfDigest, ServerResponse, SessionCloseRequest, SessionCloseResponse,
    SessionCreateRequest, SessionCreateResponse,
};
pub use raft::{
    RaftAppendReqFrame, RaftAppendRespFrame, RaftApplyReqFrame, RaftApplyRespFrame,
    RaftLogEntryWire, RaftPreVoteReqFrame, RaftPreVoteRespFrame, RaftUpdateConfReqFrame,
    RaftUpdateConfRespFrame, RaftVoteReqFrame, RaftVoteRespFrame,
};
pub use repl::{
    ReplAckFrame, ReplBatchFrame, ReplHealFetchReq, ReplHealRespFrame, ReplSnapshotFetchReq,
    ReplSnapshotFileEntry, ReplSnapshotManifest, ReplSnapshotOfferFrame, ReplSnapshotRespFrame,
    ReplSubscribeAcceptFrame, ReplSubscribeReq,
};

// Kept in scope for `mod_test`, whose glob import of this module relies on
// these names being visible here.
#[cfg(test)]
use mudu::error::ErrorCode;
#[cfg(test)]
use mudu::mudu_error;

#[cfg(test)]
mod mod_test;
