//! `protocol::mod` module.
#![allow(missing_docs)]

use crate::tuple::tuple_field_desc::TupleFieldDesc;
use crate::tuple::tuple_value::TupleValue;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use mudu_sys_contract::perf::TraceContext;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub mod format;
pub mod migrate;
pub use format::latest::HEADER_LEN;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageType {
    Handshake = 1,
    Auth = 2,
    Query = 3,
    Execute = 4,
    Batch = 5,
    Response = 6,
    Error = 7,
    Get = 8,
    Put = 9,
    RangeScan = 10,
    ProcedureInvoke = 11,
    SessionCreate = 12,
    SessionClose = 13,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandshakeRequest {
    /// Protocol frame versions supported by the client.
    pub supported_versions: Vec<u32>,
    /// Optional client capability tags.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandshakeResponse {
    /// Negotiated protocol frame version.
    pub selected_version: u32,
    /// Optional server capability tags.
    #[serde(default)]
    pub capabilities: Vec<String>,
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
            _ => Err(mudu_error!(
                ErrorCode::Parse,
                format!("unknown message type {}", value)
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FrameHeader {
    magic: u32,
    version: u32,
    message_type: MessageType,
    flags: u64,
    request_id: u64,
    trace_context: TraceContext,
    payload_len: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRequest {
    oid: u128,
    app_name: String,
    sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerResponse {
    row_desc: TupleFieldDesc,
    rows: Vec<TupleValue>,
    affected_rows: u64,
    error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    server_perf_digest: Option<ServerPerfDigest>,
}

/// Server-side per-transaction performance digest returned to the client.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ServerPerfDigest {
    pub trace_id: u64,
    pub durations_ns: [Option<u64>; mudu_sys_contract::perf::TxnStage::Count as usize],
}

impl ServerPerfDigest {
    pub fn new(trace_id: u64) -> Self {
        Self {
            trace_id,
            ..Default::default()
        }
    }

    pub fn set(&mut self, stage: mudu_sys_contract::perf::TxnStage, ns: u64) {
        self.durations_ns[stage.as_index()] = Some(ns);
    }

    pub fn get(&self, stage: mudu_sys_contract::perf::TxnStage) -> Option<u64> {
        self.durations_ns[stage.as_index()]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRequest {
    session_id: u128,
    key: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutRequest {
    session_id: u128,
    key: Vec<u8>,
    value: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeScanRequest {
    session_id: u128,
    start_key: Vec<u8>,
    end_key: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcedureInvokeRequest {
    session_id: u128,
    procedure_name: String,
    procedure_parameters: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionCreateRequest {
    config_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreateResponse {
    session_id: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCloseRequest {
    session_id: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionCloseResponse {
    closed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyValue {
    key: Vec<u8>,
    value: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetResponse {
    value: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PutResponse {
    ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RangeScanResponse {
    items: Vec<KeyValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProcedureInvokeResponse {
    result: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    server_perf_digest: Option<ServerPerfDigest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorResponse {
    #[serde(default)]
    code: u32,
    #[serde(default)]
    name: String,
    message: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    location: String,
}

#[derive(Debug, Clone)]
pub struct Frame {
    header: FrameHeader,
    payload: Vec<u8>,
}

impl Frame {
    pub fn new(message_type: MessageType, request_id: u64, payload: Vec<u8>) -> Self {
        Self::new_with_trace(message_type, request_id, TraceContext::empty(), payload)
    }

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

    pub fn encode(&self) -> Vec<u8> {
        format::encode_latest(self)
    }

    pub fn decode(buf: &[u8]) -> RS<Self> {
        format::decode(buf)
    }

    pub fn header(&self) -> &FrameHeader {
        &self.header
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn into_payload(self) -> Vec<u8> {
        self.payload
    }
}

impl FrameHeader {
    pub fn new(message_type: MessageType, request_id: u64, payload_len: u32) -> Self {
        Self::new_with_trace(message_type, request_id, TraceContext::empty(), payload_len)
    }

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

    pub fn decode_header_bytes(buf: &[u8]) -> RS<Self> {
        format::decode_header_bytes(buf)
    }

    pub fn magic(&self) -> u32 {
        self.magic
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn message_type(&self) -> MessageType {
        self.message_type
    }

    pub fn flags(&self) -> u64 {
        self.flags
    }

    pub fn sampled(&self) -> bool {
        self.trace_context.sampled
    }

    pub fn request_id(&self) -> u64 {
        self.request_id
    }

    pub fn trace_context(&self) -> TraceContext {
        self.trace_context
    }

    pub fn payload_len(&self) -> u32 {
        self.payload_len
    }
}

impl ClientRequest {
    pub fn new(app_name: impl Into<String>, sql: impl Into<String>) -> Self {
        Self {
            oid: 0,
            app_name: app_name.into(),
            sql: sql.into(),
        }
    }

    pub fn new_with_oid(oid: u128, app_name: impl Into<String>, sql: impl Into<String>) -> Self {
        Self {
            oid,
            app_name: app_name.into(),
            sql: sql.into(),
        }
    }

    pub fn oid(&self) -> u128 {
        self.oid
    }

    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    pub fn sql(&self) -> &str {
        &self.sql
    }
}

impl ServerResponse {
    pub fn new(
        row_desc: TupleFieldDesc,
        rows: Vec<TupleValue>,
        affected_rows: u64,
        error: Option<String>,
    ) -> Self {
        Self {
            row_desc,
            rows,
            affected_rows,
            error,
            server_perf_digest: None,
        }
    }

    pub fn with_server_perf_digest(mut self, digest: ServerPerfDigest) -> Self {
        self.server_perf_digest = Some(digest);
        self
    }

    pub fn server_perf_digest(&self) -> Option<&ServerPerfDigest> {
        self.server_perf_digest.as_ref()
    }

    pub fn row_desc(&self) -> &TupleFieldDesc {
        &self.row_desc
    }

    pub fn rows(&self) -> &[TupleValue] {
        &self.rows
    }

    pub fn affected_rows(&self) -> u64 {
        self.affected_rows
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

impl GetRequest {
    pub fn new(session_id: u128, key: Vec<u8>) -> Self {
        Self { session_id, key }
    }

    pub fn session_id(&self) -> u128 {
        self.session_id
    }

    pub fn key(&self) -> &[u8] {
        &self.key
    }
}

impl PutRequest {
    pub fn new(session_id: u128, key: Vec<u8>, value: Vec<u8>) -> Self {
        Self {
            session_id,
            key,
            value,
        }
    }

    pub fn session_id(&self) -> u128 {
        self.session_id
    }

    pub fn key(&self) -> &[u8] {
        &self.key
    }

    pub fn value(&self) -> &[u8] {
        &self.value
    }

    pub fn into_parts(self) -> (Vec<u8>, Vec<u8>) {
        (self.key, self.value)
    }
}

impl RangeScanRequest {
    pub fn new(session_id: u128, start_key: Vec<u8>, end_key: Vec<u8>) -> Self {
        Self {
            session_id,
            start_key,
            end_key,
        }
    }

    pub fn session_id(&self) -> u128 {
        self.session_id
    }

    pub fn start_key(&self) -> &[u8] {
        &self.start_key
    }

    pub fn end_key(&self) -> &[u8] {
        &self.end_key
    }
}

impl ProcedureInvokeRequest {
    pub fn new(
        session_id: u128,
        procedure_name: impl Into<String>,
        procedure_parameters: Vec<u8>,
    ) -> Self {
        Self {
            session_id,
            procedure_name: procedure_name.into(),
            procedure_parameters,
        }
    }

    pub fn session_id(&self) -> u128 {
        self.session_id
    }

    pub fn procedure_name(&self) -> &str {
        &self.procedure_name
    }

    pub fn procedure_parameters(&self) -> &[u8] {
        &self.procedure_parameters
    }

    pub fn procedure_parameters_owned(&self) -> Vec<u8> {
        self.procedure_parameters.clone()
    }
}

impl SessionCreateRequest {
    pub fn new(config_json: Option<String>) -> Self {
        Self { config_json }
    }

    pub fn config_json(&self) -> Option<&str> {
        self.config_json.as_deref()
    }
}

impl KeyValue {
    pub fn new(key: Vec<u8>, value: Vec<u8>) -> Self {
        Self { key, value }
    }

    pub fn key(&self) -> &[u8] {
        &self.key
    }

    pub fn value(&self) -> &[u8] {
        &self.value
    }
}

impl GetResponse {
    pub fn new(value: Option<Vec<u8>>) -> Self {
        Self { value }
    }

    pub fn value(&self) -> Option<&[u8]> {
        self.value.as_deref()
    }

    pub fn into_value(self) -> Option<Vec<u8>> {
        self.value
    }
}

impl PutResponse {
    pub fn new(ok: bool) -> Self {
        Self { ok }
    }

    pub fn ok(&self) -> bool {
        self.ok
    }
}

impl RangeScanResponse {
    pub fn new(items: Vec<KeyValue>) -> Self {
        Self { items }
    }

    pub fn items(&self) -> &[KeyValue] {
        &self.items
    }

    pub fn into_items(self) -> Vec<KeyValue> {
        self.items
    }
}

impl ProcedureInvokeResponse {
    pub fn new(result: Vec<u8>) -> Self {
        Self {
            result,
            server_perf_digest: None,
        }
    }

    pub fn result(&self) -> &[u8] {
        &self.result
    }

    pub fn into_result(self) -> Vec<u8> {
        self.result
    }

    pub fn with_server_perf_digest(mut self, digest: ServerPerfDigest) -> Self {
        self.server_perf_digest = Some(digest);
        self
    }

    pub fn server_perf_digest(&self) -> Option<&ServerPerfDigest> {
        self.server_perf_digest.as_ref()
    }
}

impl SessionCreateResponse {
    pub fn new(session_id: u128) -> Self {
        Self { session_id }
    }

    pub fn session_id(&self) -> u128 {
        self.session_id
    }
}

impl SessionCloseRequest {
    pub fn new(session_id: u128) -> Self {
        Self { session_id }
    }

    pub fn session_id(&self) -> u128 {
        self.session_id
    }
}

impl SessionCloseResponse {
    pub fn new(closed: bool) -> Self {
        Self { closed }
    }

    pub fn closed(&self) -> bool {
        self.closed
    }
}

impl ErrorResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::Internal.to_u32(),
            name: "Internal".to_string(),
            message: message.into(),
            source: String::new(),
            location: String::new(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn code(&self) -> u32 {
        self.code
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn location(&self) -> &str {
        &self.location
    }

    pub fn from_merror(error: &MuduError) -> Self {
        Self {
            code: error.ec().to_u32(),
            name: format!("{:?}", error.ec()),
            message: error.message().to_string(),
            source: error.err_src().to_json_str(),
            location: error.loc().to_string(),
        }
    }
}

pub fn encode_handshake_request(request_id: u64, request: &HandshakeRequest) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode handshake request error")?;
    Ok(Frame::new(MessageType::Handshake, request_id, payload).encode())
}

pub fn decode_handshake_request(frame: &Frame) -> RS<HandshakeRequest> {
    decode_payload(frame.payload(), "decode handshake request error")
}

pub fn encode_handshake_response(request_id: u64, response: &HandshakeResponse) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode handshake response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

pub fn decode_handshake_response(frame: &Frame) -> RS<HandshakeResponse> {
    decode_payload(frame.payload(), "decode handshake response error")
}

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

pub fn encode_client_request_with_message_type_and_trace(
    message_type: MessageType,
    request_id: u64,
    trace_context: TraceContext,
    request: &ClientRequest,
) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode client request error")?;
    Ok(Frame::new_with_trace(message_type, request_id, trace_context, payload).encode())
}

pub fn encode_client_request(request_id: u64, request: &ClientRequest) -> RS<Vec<u8>> {
    encode_client_request_with_message_type(MessageType::Query, request_id, request)
}

pub fn decode_client_request(frame: &Frame) -> RS<ClientRequest> {
    decode_payload(frame.payload(), "decode client request error")
}

pub fn encode_batch_request(request_id: u64, request: &ClientRequest) -> RS<Vec<u8>> {
    encode_client_request_with_message_type(MessageType::Batch, request_id, request)
}

pub fn encode_server_response(request_id: u64, response: &ServerResponse) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode server response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

pub fn decode_server_response(frame: &Frame) -> RS<ServerResponse> {
    decode_payload(frame.payload(), "decode server response error")
}

pub fn encode_get_request(request_id: u64, request: &GetRequest) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode get request error")?;
    Ok(Frame::new(MessageType::Get, request_id, payload).encode())
}

pub fn decode_get_request(frame: &Frame) -> RS<GetRequest> {
    decode_payload(frame.payload(), "decode get request error")
}

pub fn decode_get_response(frame: &Frame) -> RS<GetResponse> {
    decode_payload(frame.payload(), "decode get response error")
}

pub fn encode_put_request(request_id: u64, request: &PutRequest) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode put request error")?;
    Ok(Frame::new(MessageType::Put, request_id, payload).encode())
}

pub fn decode_put_request(frame: &Frame) -> RS<PutRequest> {
    decode_payload(frame.payload(), "decode put request error")
}

pub fn decode_put_response(frame: &Frame) -> RS<PutResponse> {
    decode_payload(frame.payload(), "decode put response error")
}

pub fn encode_range_scan_request(request_id: u64, request: &RangeScanRequest) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode range scan request error")?;
    Ok(Frame::new(MessageType::RangeScan, request_id, payload).encode())
}

pub fn decode_range_scan_request(frame: &Frame) -> RS<RangeScanRequest> {
    decode_payload(frame.payload(), "decode range scan request error")
}

pub fn encode_procedure_invoke_request(
    request_id: u64,
    request: &ProcedureInvokeRequest,
) -> RS<Vec<u8>> {
    encode_procedure_invoke_request_with_trace(request_id, TraceContext::empty(), request)
}

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

pub fn encode_session_create_request(
    request_id: u64,
    request: &SessionCreateRequest,
) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode session create request error")?;
    Ok(Frame::new(MessageType::SessionCreate, request_id, payload).encode())
}

pub fn encode_session_close_request(request_id: u64, request: &SessionCloseRequest) -> RS<Vec<u8>> {
    let payload = encode_payload(request, "encode session close request error")?;
    Ok(Frame::new(MessageType::SessionClose, request_id, payload).encode())
}

pub fn decode_range_scan_response(frame: &Frame) -> RS<RangeScanResponse> {
    decode_payload(frame.payload(), "decode range scan response error")
}

pub fn decode_procedure_invoke_request(frame: &Frame) -> RS<ProcedureInvokeRequest> {
    decode_payload(frame.payload(), "decode procedure invoke request error")
}

pub fn decode_session_create_response(frame: &Frame) -> RS<SessionCreateResponse> {
    decode_payload(frame.payload(), "decode session create response error")
}

pub fn decode_session_create_request(frame: &Frame) -> RS<SessionCreateRequest> {
    if frame.payload().is_empty() {
        return Ok(SessionCreateRequest::default());
    }
    decode_payload(frame.payload(), "decode session create request error")
}

pub fn decode_session_close_request(frame: &Frame) -> RS<SessionCloseRequest> {
    decode_payload(frame.payload(), "decode session close request error")
}

pub fn encode_get_response(request_id: u64, response: &GetResponse) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode get response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

pub fn encode_put_response(request_id: u64, response: &PutResponse) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode put response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

pub fn encode_range_scan_response(request_id: u64, response: &RangeScanResponse) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode range scan response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

pub fn encode_procedure_invoke_response(
    request_id: u64,
    response: &ProcedureInvokeResponse,
) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode procedure invoke response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

pub fn encode_session_create_response(
    request_id: u64,
    response: &SessionCreateResponse,
) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode session create response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

pub fn encode_session_close_response(
    request_id: u64,
    response: &SessionCloseResponse,
) -> RS<Vec<u8>> {
    let payload = encode_payload(response, "encode session close response error")?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

pub fn decode_procedure_invoke_response(frame: &Frame) -> RS<ProcedureInvokeResponse> {
    decode_payload(frame.payload(), "decode procedure invoke response error")
}

pub fn decode_session_close_response(frame: &Frame) -> RS<SessionCloseResponse> {
    decode_payload(frame.payload(), "decode session close response error")
}

pub fn encode_error_response(request_id: u64, message: impl Into<String>) -> RS<Vec<u8>> {
    let payload = encode_payload(&ErrorResponse::new(message), "encode error response error")?;
    Ok(Frame::new(MessageType::Error, request_id, payload).encode())
}

#[rustfmt::skip]
pub fn encode_merror_response(request_id: u64, error: &MuduError) -> RS<Vec<u8>> {
    let payload = encode_payload(&ErrorResponse::from_merror(error), "encode merror response error")?;
    Ok(Frame::new(MessageType::Error, request_id, payload).encode())
}

pub fn decode_error_response(frame: &Frame) -> RS<ErrorResponse> {
    decode_payload(frame.payload(), "decode error response error")
}

fn encode_payload<T: Serialize>(value: &T, err_msg: &'static str) -> RS<Vec<u8>> {
    rmp_serde::to_vec(value).map_err(|e| mudu_error!(ErrorCode::Encode, err_msg, e))
}

fn decode_payload<T: DeserializeOwned>(payload: &[u8], err_msg: &'static str) -> RS<T> {
    rmp_serde::from_slice(payload).map_err(|e| mudu_error!(ErrorCode::Decode, err_msg, e))
}

#[cfg(test)]
mod mod_test;
