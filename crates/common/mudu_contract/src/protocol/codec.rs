//! Frame-level encode/decode helpers for each message type.

use super::error::{ErrorResponse, LeaderHint};
use super::frame::{Frame, MessageType};
use super::message::{
    ClientRequest, GetRequest, GetResponse, HandshakeRequest, HandshakeResponse,
    ProcedureInvokeRequest, ProcedureInvokeResponse, PutRequest, PutResponse, RangeScanRequest,
    RangeScanResponse, ServerResponse, SessionCloseRequest, SessionCloseResponse,
    SessionCreateRequest, SessionCreateResponse,
};
use super::raft::{
    RaftAppendReqFrame, RaftAppendRespFrame, RaftApplyReqFrame, RaftApplyRespFrame,
    RaftPreVoteReqFrame, RaftPreVoteRespFrame, RaftUpdateConfReqFrame, RaftUpdateConfRespFrame,
    RaftVoteReqFrame, RaftVoteRespFrame,
};
use super::repl::{
    ReplAckFrame, ReplBatchFrame, ReplHealFetchReq, ReplHealRespFrame, ReplSnapshotFetchReq,
    ReplSnapshotOfferFrame, ReplSnapshotRespFrame, ReplSubscribeAcceptFrame, ReplSubscribeReq,
};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use mudu_sys_contract::perf::TraceContext;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Encodes a handshake request into a `Handshake` frame.
pub fn encode_handshake_request(request_id: u64, request: &HandshakeRequest) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode handshake request error")?;
    Ok(Frame::new(MessageType::Handshake, request_id, payload).encode())
}

/// Decodes a handshake request from a `Handshake` frame.
pub fn decode_handshake_request(frame: &Frame) -> RS<HandshakeRequest> {
    decode_payload(frame.payload(), "decode handshake request error")
}

/// Encodes a handshake response into a `Response` frame.
pub fn encode_handshake_response(request_id: u64, response: &HandshakeResponse) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode handshake response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

/// Decodes a handshake response from a `Response` frame.
pub fn decode_handshake_response(frame: &Frame) -> RS<HandshakeResponse> {
    decode_payload(frame.payload(), "decode handshake response error")
}

/// Encodes a client request into a frame of the given message type.
pub fn encode_client_request_with_message_type(
    message_type: MessageType,
    request_id: u64,
    request: &ClientRequest,
) -> RS<Vec<u8>> {
    encode_client_request_with_message_type_and_trace(
        message_type,
        request_id,
        TraceContext::empty(),
        request,
    )
}

/// Encodes a client request into a frame of the given message type, carrying a trace context.
pub fn encode_client_request_with_message_type_and_trace(
    message_type: MessageType,
    request_id: u64,
    trace_context: TraceContext,
    request: &ClientRequest,
) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode client request error")?;
    Ok(Frame::new_with_trace(message_type, request_id, trace_context, payload).encode())
}

/// Encodes a client request into a `Query` frame.
pub fn encode_client_request(request_id: u64, request: &ClientRequest) -> RS<Vec<u8>> {
    encode_client_request_with_message_type(MessageType::Query, request_id, request)
}

/// Decodes a client request from a `Query`/`Execute`/`Batch` frame.
pub fn decode_client_request(frame: &Frame) -> RS<ClientRequest> {
    decode_payload(frame.payload(), "decode client request error")
}

/// Encodes a client request into a `Batch` frame.
pub fn encode_batch_request(request_id: u64, request: &ClientRequest) -> RS<Vec<u8>> {
    encode_client_request_with_message_type(MessageType::Batch, request_id, request)
}

/// Encodes a server response into a `Response` frame.
pub fn encode_server_response(request_id: u64, response: &ServerResponse) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode server response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

/// Decodes a server response from a `Response` frame.
pub fn decode_server_response(frame: &Frame) -> RS<ServerResponse> {
    decode_payload(frame.payload(), "decode server response error")
}

/// Encodes a key-value get request into a `Get` frame.
pub fn encode_get_request(request_id: u64, request: &GetRequest) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode get request error")?;
    Ok(Frame::new(MessageType::Get, request_id, payload).encode())
}

/// Decodes a key-value get request from a `Get` frame.
pub fn decode_get_request(frame: &Frame) -> RS<GetRequest> {
    decode_payload(frame.payload(), "decode get request error")
}

/// Decodes a key-value get response from a `Response` frame.
pub fn decode_get_response(frame: &Frame) -> RS<GetResponse> {
    decode_payload(frame.payload(), "decode get response error")
}

/// Encodes a key-value put request into a `Put` frame.
pub fn encode_put_request(request_id: u64, request: &PutRequest) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode put request error")?;
    Ok(Frame::new(MessageType::Put, request_id, payload).encode())
}

/// Decodes a key-value put request from a `Put` frame.
pub fn decode_put_request(frame: &Frame) -> RS<PutRequest> {
    decode_payload(frame.payload(), "decode put request error")
}

/// Decodes a key-value put response from a `Response` frame.
pub fn decode_put_response(frame: &Frame) -> RS<PutResponse> {
    decode_payload(frame.payload(), "decode put response error")
}

/// Encodes a key-value range scan request into a `RangeScan` frame.
pub fn encode_range_scan_request(request_id: u64, request: &RangeScanRequest) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode range scan request error")?;
    Ok(Frame::new(MessageType::RangeScan, request_id, payload).encode())
}

/// Decodes a key-value range scan request from a `RangeScan` frame.
pub fn decode_range_scan_request(frame: &Frame) -> RS<RangeScanRequest> {
    decode_payload(frame.payload(), "decode range scan request error")
}

/// Encodes a procedure invoke request into a `ProcedureInvoke` frame.
pub fn encode_procedure_invoke_request(
    request_id: u64,
    request: &ProcedureInvokeRequest,
) -> RS<Vec<u8>> {
    encode_procedure_invoke_request_with_trace(request_id, TraceContext::empty(), request)
}

/// Encodes a procedure invoke request into a `ProcedureInvoke` frame, carrying a trace context.
pub fn encode_procedure_invoke_request_with_trace(
    request_id: u64,
    trace_context: TraceContext,
    request: &ProcedureInvokeRequest,
) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode procedure invoke request error")?;
    Ok(Frame::new_with_trace(
        MessageType::ProcedureInvoke,
        request_id,
        trace_context,
        payload,
    )
    .encode())
}

/// Encodes a session create request into a `SessionCreate` frame.
pub fn encode_session_create_request(
    request_id: u64,
    request: &SessionCreateRequest,
) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode session create request error")?;
    Ok(Frame::new(MessageType::SessionCreate, request_id, payload).encode())
}

/// Encodes a session close request into a `SessionClose` frame.
pub fn encode_session_close_request(request_id: u64, request: &SessionCloseRequest) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode session close request error")?;
    Ok(Frame::new(MessageType::SessionClose, request_id, payload).encode())
}

/// Decodes a key-value range scan response from a `Response` frame.
pub fn decode_range_scan_response(frame: &Frame) -> RS<RangeScanResponse> {
    decode_payload(frame.payload(), "decode range scan response error")
}

/// Decodes a procedure invoke request from a `ProcedureInvoke` frame.
pub fn decode_procedure_invoke_request(frame: &Frame) -> RS<ProcedureInvokeRequest> {
    decode_payload(frame.payload(), "decode procedure invoke request error")
}

/// Decodes a session create response from a `Response` frame.
pub fn decode_session_create_response(frame: &Frame) -> RS<SessionCreateResponse> {
    decode_payload(frame.payload(), "decode session create response error")
}

/// Decodes a session create request from a `SessionCreate` frame; an empty
/// payload decodes to the default request.
pub fn decode_session_create_request(frame: &Frame) -> RS<SessionCreateRequest> {
    if frame.payload().is_empty() {
        return Ok(SessionCreateRequest::default());
    }
    decode_payload(frame.payload(), "decode session create request error")
}

/// Decodes a session close request from a `SessionClose` frame.
pub fn decode_session_close_request(frame: &Frame) -> RS<SessionCloseRequest> {
    decode_payload(frame.payload(), "decode session close request error")
}

/// Encodes a key-value get response into a `Response` frame.
pub fn encode_get_response(request_id: u64, response: &GetResponse) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode get response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

/// Encodes a key-value put response into a `Response` frame.
pub fn encode_put_response(request_id: u64, response: &PutResponse) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode put response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

/// Encodes a key-value range scan response into a `Response` frame.
pub fn encode_range_scan_response(request_id: u64, response: &RangeScanResponse) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode range scan response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

/// Encodes a procedure invoke response into a `Response` frame.
pub fn encode_procedure_invoke_response(
    request_id: u64,
    response: &ProcedureInvokeResponse,
) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode procedure invoke response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

/// Encodes a session create response into a `Response` frame.
pub fn encode_session_create_response(
    request_id: u64,
    response: &SessionCreateResponse,
) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode session create response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

/// Encodes a session close response into a `Response` frame.
pub fn encode_session_close_response(
    request_id: u64,
    response: &SessionCloseResponse,
) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode session close response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

/// Decodes a procedure invoke response from a `Response` frame.
pub fn decode_procedure_invoke_response(frame: &Frame) -> RS<ProcedureInvokeResponse> {
    decode_payload(frame.payload(), "decode procedure invoke response error")
}

/// Decodes a session close response from a `Response` frame.
pub fn decode_session_close_response(frame: &Frame) -> RS<SessionCloseResponse> {
    decode_payload(frame.payload(), "decode session close response error")
}

/// Encodes an error message into an `Error` frame.
pub fn encode_error_response(request_id: u64, message: impl Into<String>) -> RS<Vec<u8>> {
    let payload = encode_payload(&ErrorResponse::new(message), "encode error response error")?;
    Ok(Frame::new(MessageType::Error, request_id, payload).encode())
}

/// Encodes a Mudu error into an `Error` frame.
#[rustfmt::skip]
pub fn encode_merror_response(request_id: u64, error: &MuduError) -> RS<Vec<u8>> {
    let payload = encode_payload(&ErrorResponse::from_merror(error), "encode merror response error")?;
    Ok(Frame::new(MessageType::Error, request_id, payload).encode())
}

/// Encodes an error response that carries a replication leader hint, so a
/// client (or replica) that landed on the wrong node can redirect to the
/// current primary.
#[rustfmt::skip]
pub fn encode_merror_response_with_leader_hint(
    request_id: u64,
    error: &MuduError,
    leader_hint: LeaderHint,
) -> RS<Vec<u8>> {
    let payload = encode_payload(
        &ErrorResponse::from_merror(error).with_leader_hint(leader_hint),
        "encode merror response with leader hint error",
    )?;
    Ok(Frame::new(MessageType::Error, request_id, payload).encode())
}

/// Decodes an error response from an `Error` frame.
pub fn decode_error_response(frame: &Frame) -> RS<ErrorResponse> {
    decode_payload(frame.payload(), "decode error response error")
}

/// Encodes a replication subscribe request into a `ReplSubscribe` frame.
pub fn encode_repl_subscribe_request(request_id: u64, request: &ReplSubscribeReq) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode repl subscribe request error")?;
    Ok(Frame::new(MessageType::ReplSubscribe, request_id, payload).encode())
}

/// Decodes a replication subscribe request from a `ReplSubscribe` frame.
pub fn decode_repl_subscribe_request(frame: &Frame) -> RS<ReplSubscribeReq> {
    decode_payload(frame.payload(), "decode repl subscribe request error")
}

/// Encodes a subscription acceptance into a `ReplSubscribeAccept` frame.
pub fn encode_repl_subscribe_accept_frame(
    request_id: u64,
    accept: &ReplSubscribeAcceptFrame,
) -> RS<Vec<u8>> {
    let payload = encode_payload(accept, "encode repl subscribe accept frame error")?;
    Ok(Frame::new(MessageType::ReplSubscribeAccept, request_id, payload).encode())
}

/// Decodes a subscription acceptance from a `ReplSubscribeAccept` frame.
pub fn decode_repl_subscribe_accept_frame(frame: &Frame) -> RS<ReplSubscribeAcceptFrame> {
    decode_payload(frame.payload(), "decode repl subscribe accept frame error")
}

/// Encodes a replication batch into a `ReplBatch` frame.
pub fn encode_repl_batch_frame(request_id: u64, batch: &ReplBatchFrame) -> RS<Vec<u8>> {
    let payload = encode_payload(batch, "encode repl batch frame error")?;
    Ok(Frame::new(MessageType::ReplBatch, request_id, payload).encode())
}

/// Decodes a replication batch from a `ReplBatch` frame.
pub fn decode_repl_batch_frame(frame: &Frame) -> RS<ReplBatchFrame> {
    decode_payload(frame.payload(), "decode repl batch frame error")
}

/// Encodes a replication acknowledgement into a `ReplAck` frame.
pub fn encode_repl_ack_frame(request_id: u64, ack: &ReplAckFrame) -> RS<Vec<u8>> {
    let payload = encode_payload(ack, "encode repl ack frame error")?;
    Ok(Frame::new(MessageType::ReplAck, request_id, payload).encode())
}

/// Decodes a replication acknowledgement from a `ReplAck` frame.
pub fn decode_repl_ack_frame(frame: &Frame) -> RS<ReplAckFrame> {
    decode_payload(frame.payload(), "decode repl ack frame error")
}

/// Encodes a replication heal fetch request into a `ReplHealFetch` frame.
pub fn encode_repl_heal_fetch_request(request_id: u64, request: &ReplHealFetchReq) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode repl heal fetch request error")?;
    Ok(Frame::new(MessageType::ReplHealFetch, request_id, payload).encode())
}

/// Decodes a replication heal fetch request from a `ReplHealFetch` frame.
pub fn decode_repl_heal_fetch_request(frame: &Frame) -> RS<ReplHealFetchReq> {
    decode_payload(frame.payload(), "decode repl heal fetch request error")
}

/// Encodes a replication heal response into a `ReplHealResp` frame.
pub fn encode_repl_heal_response_frame(
    request_id: u64,
    response: &ReplHealRespFrame,
) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode repl heal response frame error")?;
    Ok(Frame::new(MessageType::ReplHealResp, request_id, payload).encode())
}

/// Decodes a replication heal response from a `ReplHealResp` frame.
pub fn decode_repl_heal_response_frame(frame: &Frame) -> RS<ReplHealRespFrame> {
    decode_payload(frame.payload(), "decode repl heal response frame error")
}

/// Encodes a snapshot offer into a `ReplSnapshotOffer` frame.
pub fn encode_repl_snapshot_offer_frame(
    request_id: u64,
    offer: &ReplSnapshotOfferFrame,
) -> RS<Vec<u8>> {
    let payload = encode_payload(offer, "encode repl snapshot offer frame error")?;
    Ok(Frame::new(MessageType::ReplSnapshotOffer, request_id, payload).encode())
}

/// Decodes a snapshot offer from a `ReplSnapshotOffer` frame.
pub fn decode_repl_snapshot_offer_frame(frame: &Frame) -> RS<ReplSnapshotOfferFrame> {
    decode_payload(frame.payload(), "decode repl snapshot offer frame error")
}

/// Encodes a snapshot fetch request into a `ReplSnapshotFetch` frame.
pub fn encode_repl_snapshot_fetch_request(
    request_id: u64,
    request: &ReplSnapshotFetchReq,
) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode repl snapshot fetch request error")?;
    Ok(Frame::new(MessageType::ReplSnapshotFetch, request_id, payload).encode())
}

/// Decodes a snapshot fetch request from a `ReplSnapshotFetch` frame.
pub fn decode_repl_snapshot_fetch_request(frame: &Frame) -> RS<ReplSnapshotFetchReq> {
    decode_payload(frame.payload(), "decode repl snapshot fetch request error")
}

/// Encodes a snapshot fetch response into a `ReplSnapshotResp` frame.
pub fn encode_repl_snapshot_resp_frame(
    request_id: u64,
    response: &ReplSnapshotRespFrame,
) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode repl snapshot resp frame error")?;
    Ok(Frame::new(MessageType::ReplSnapshotResp, request_id, payload).encode())
}

/// Decodes a snapshot fetch response from a `ReplSnapshotResp` frame.
pub fn decode_repl_snapshot_resp_frame(frame: &Frame) -> RS<ReplSnapshotRespFrame> {
    decode_payload(frame.payload(), "decode repl snapshot resp frame error")
}

/// Encodes a raft pre-vote request into a `RaftPreVoteReq` frame.
pub fn encode_raft_pre_vote_request(request_id: u64, request: &RaftPreVoteReqFrame) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode raft pre-vote request error")?;
    Ok(Frame::new(MessageType::RaftPreVoteReq, request_id, payload).encode())
}

/// Decodes a raft pre-vote request from a `RaftPreVoteReq` frame.
pub fn decode_raft_pre_vote_request(frame: &Frame) -> RS<RaftPreVoteReqFrame> {
    decode_payload(frame.payload(), "decode raft pre-vote request error")
}

/// Encodes a raft pre-vote response into a `RaftPreVoteResp` frame.
pub fn encode_raft_pre_vote_response(
    request_id: u64,
    response: &RaftPreVoteRespFrame,
) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode raft pre-vote response error")?;
    Ok(Frame::new(MessageType::RaftPreVoteResp, request_id, payload).encode())
}

/// Decodes a raft pre-vote response from a `RaftPreVoteResp` frame.
pub fn decode_raft_pre_vote_response(frame: &Frame) -> RS<RaftPreVoteRespFrame> {
    decode_payload(frame.payload(), "decode raft pre-vote response error")
}

/// Encodes a raft vote request into a `RaftVoteReq` frame.
pub fn encode_raft_vote_request(request_id: u64, request: &RaftVoteReqFrame) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode raft vote request error")?;
    Ok(Frame::new(MessageType::RaftVoteReq, request_id, payload).encode())
}

/// Decodes a raft vote request from a `RaftVoteReq` frame.
pub fn decode_raft_vote_request(frame: &Frame) -> RS<RaftVoteReqFrame> {
    decode_payload(frame.payload(), "decode raft vote request error")
}

/// Encodes a raft vote response into a `RaftVoteResp` frame.
pub fn encode_raft_vote_response(request_id: u64, response: &RaftVoteRespFrame) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode raft vote response error")?;
    Ok(Frame::new(MessageType::RaftVoteResp, request_id, payload).encode())
}

/// Decodes a raft vote response from a `RaftVoteResp` frame.
pub fn decode_raft_vote_response(frame: &Frame) -> RS<RaftVoteRespFrame> {
    decode_payload(frame.payload(), "decode raft vote response error")
}

/// Encodes a raft append-entries request into a `RaftAppendReq` frame.
pub fn encode_raft_append_request(request_id: u64, request: &RaftAppendReqFrame) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode raft append request error")?;
    Ok(Frame::new(MessageType::RaftAppendReq, request_id, payload).encode())
}

/// Decodes a raft append-entries request from a `RaftAppendReq` frame.
pub fn decode_raft_append_request(frame: &Frame) -> RS<RaftAppendReqFrame> {
    decode_payload(frame.payload(), "decode raft append request error")
}

/// Encodes a raft append-entries response into a `RaftAppendResp` frame.
pub fn encode_raft_append_response(request_id: u64, response: &RaftAppendRespFrame) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode raft append response error")?;
    Ok(Frame::new(MessageType::RaftAppendResp, request_id, payload).encode())
}

/// Decodes a raft append-entries response from a `RaftAppendResp` frame.
pub fn decode_raft_append_response(frame: &Frame) -> RS<RaftAppendRespFrame> {
    decode_payload(frame.payload(), "decode raft append response error")
}

/// Encodes a raft apply-snapshot request into a `RaftApplyReq` frame (reserved).
pub fn encode_raft_apply_request(request_id: u64, request: &RaftApplyReqFrame) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode raft apply request error")?;
    Ok(Frame::new(MessageType::RaftApplyReq, request_id, payload).encode())
}

/// Decodes a raft apply-snapshot request from a `RaftApplyReq` frame (reserved).
pub fn decode_raft_apply_request(frame: &Frame) -> RS<RaftApplyReqFrame> {
    decode_payload(frame.payload(), "decode raft apply request error")
}

/// Encodes a raft apply-snapshot response into a `RaftApplyResp` frame (reserved).
pub fn encode_raft_apply_response(request_id: u64, response: &RaftApplyRespFrame) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode raft apply response error")?;
    Ok(Frame::new(MessageType::RaftApplyResp, request_id, payload).encode())
}

/// Decodes a raft apply-snapshot response from a `RaftApplyResp` frame (reserved).
pub fn decode_raft_apply_response(frame: &Frame) -> RS<RaftApplyRespFrame> {
    decode_payload(frame.payload(), "decode raft apply response error")
}

/// Encodes a raft configuration-sync request into a `RaftUpdateConfReq` frame.
pub fn encode_raft_update_conf_request(
    request_id: u64,
    request: &RaftUpdateConfReqFrame,
) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode raft update conf request error")?;
    Ok(Frame::new(MessageType::RaftUpdateConfReq, request_id, payload).encode())
}

/// Decodes a raft configuration-sync request from a `RaftUpdateConfReq` frame.
pub fn decode_raft_update_conf_request(frame: &Frame) -> RS<RaftUpdateConfReqFrame> {
    decode_payload(frame.payload(), "decode raft update conf request error")
}

/// Encodes a raft configuration-sync response into a `RaftUpdateConfResp` frame.
pub fn encode_raft_update_conf_response(
    request_id: u64,
    response: &RaftUpdateConfRespFrame,
) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode raft update conf response error")?;
    Ok(Frame::new(MessageType::RaftUpdateConfResp, request_id, payload).encode())
}

/// Decodes a raft configuration-sync response from a `RaftUpdateConfResp` frame.
pub fn decode_raft_update_conf_response(frame: &Frame) -> RS<RaftUpdateConfRespFrame> {
    decode_payload(frame.payload(), "decode raft update conf response error")
}

fn encode_payload<T: Serialize>(value: &T, err_msg: &'static str) -> RS<Vec<u8>> {
    rmp_serde::to_vec(value).map_err(|e| mudu_error!(ErrorCode::Encode, err_msg, e))
}

fn decode_payload<T: DeserializeOwned>(payload: &[u8], err_msg: &'static str) -> RS<T> {
    rmp_serde::from_slice(payload).map_err(|e| mudu_error!(ErrorCode::Decode, err_msg, e))
}
