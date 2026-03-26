use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use serde::{Deserialize, Serialize};

pub const HEADER_LEN: usize = 20;
const MAGIC: u16 = 0x4d44;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageType {
    Handshake = 1,
    Auth = 2,
    Query = 3,
    Execute = 4,
    Response = 5,
    Error = 6,
    Get = 7,
    Put = 8,
    RangeScan = 9,
    ProcedureInvoke = 10,
    SessionCreate = 11,
    SessionClose = 12,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameHeader {
    magic: u16,
    version: u16,
    message_type: MessageType,
    flags: u16,
    request_id: u64,
    payload_len: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRequest {
    app_name: String,
    sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerResponse {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    affected_rows: u64,
    error: Option<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorResponse {
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    header: FrameHeader,
    payload: Vec<u8>,
}

impl Frame {
    pub fn new(message_type: MessageType, request_id: u64, payload: Vec<u8>) -> Self {
        Self {
            header: FrameHeader {
                magic: MAGIC,
                version: 1,
                message_type,
                flags: 0,
                request_id,
                payload_len: payload.len() as u32,
            },
            payload,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + self.payload.len());
        out.extend_from_slice(&self.header.magic.to_be_bytes());
        out.extend_from_slice(&self.header.version.to_be_bytes());
        out.extend_from_slice(&(self.header.message_type as u16).to_be_bytes());
        out.extend_from_slice(&self.header.flags.to_be_bytes());
        out.extend_from_slice(&self.header.request_id.to_be_bytes());
        out.extend_from_slice(&self.header.payload_len.to_be_bytes());
        out.extend_from_slice(&self.payload);
        out
    }

    pub fn decode(buf: &[u8]) -> RS<Self> {
        if buf.len() < HEADER_LEN {
            return Err(m_error!(EC::ParseErr, "frame header is incomplete"));
        }
        let magic = u16::from_be_bytes([buf[0], buf[1]]);
        if magic != MAGIC {
            return Err(m_error!(EC::ParseErr, "invalid frame magic"));
        }
        let version = u16::from_be_bytes([buf[2], buf[3]]);
        let message_type = match u16::from_be_bytes([buf[4], buf[5]]) {
            1 => MessageType::Handshake,
            2 => MessageType::Auth,
            3 => MessageType::Query,
            4 => MessageType::Execute,
            5 => MessageType::Response,
            6 => MessageType::Error,
            7 => MessageType::Get,
            8 => MessageType::Put,
            9 => MessageType::RangeScan,
            10 => MessageType::ProcedureInvoke,
            11 => MessageType::SessionCreate,
            12 => MessageType::SessionClose,
            _ => return Err(m_error!(EC::ParseErr, "unknown message type")),
        };
        let flags = u16::from_be_bytes([buf[6], buf[7]]);
        let request_id = u64::from_be_bytes([
            buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
        ]);
        let payload_len = u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]);
        let total_len = HEADER_LEN + payload_len as usize;
        if buf.len() < total_len {
            return Err(m_error!(EC::ParseErr, "frame payload is incomplete"));
        }
        Ok(Self {
            header: FrameHeader {
                magic,
                version,
                message_type,
                flags,
                request_id,
                payload_len,
            },
            payload: buf[HEADER_LEN..total_len].to_vec(),
        })
    }

    pub fn header(&self) -> &FrameHeader {
        &self.header
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

impl FrameHeader {
    pub fn magic(&self) -> u16 {
        self.magic
    }

    pub fn version(&self) -> u16 {
        self.version
    }

    pub fn message_type(&self) -> MessageType {
        self.message_type
    }

    pub fn flags(&self) -> u16 {
        self.flags
    }

    pub fn request_id(&self) -> u64 {
        self.request_id
    }

    pub fn payload_len(&self) -> u32 {
        self.payload_len
    }
}

impl ClientRequest {
    pub fn new(app_name: impl Into<String>, sql: impl Into<String>) -> Self {
        Self {
            app_name: app_name.into(),
            sql: sql.into(),
        }
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
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
        affected_rows: u64,
        error: Option<String>,
    ) -> Self {
        Self {
            columns,
            rows,
            affected_rows,
            error,
        }
    }

    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    pub fn rows(&self) -> &[Vec<String>] {
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
        Self { result }
    }

    pub fn result(&self) -> &[u8] {
        &self.result
    }

    pub fn into_result(self) -> Vec<u8> {
        self.result
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
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

pub fn encode_client_request_with_message_type(
    message_type: MessageType,
    request_id: u64,
    request: &ClientRequest,
) -> RS<Vec<u8>> {
    let payload = serde_json::to_vec(request)
        .map_err(|e| m_error!(EC::EncodeErr, "encode client request error", e))?;
    Ok(Frame::new(message_type, request_id, payload).encode())
}

pub fn encode_client_request(request_id: u64, request: &ClientRequest) -> RS<Vec<u8>> {
    encode_client_request_with_message_type(MessageType::Query, request_id, request)
}

pub fn decode_client_request(frame: &Frame) -> RS<ClientRequest> {
    serde_json::from_slice(frame.payload())
        .map_err(|e| m_error!(EC::DecodeErr, "decode client request error", e))
}

pub fn encode_server_response(request_id: u64, response: &ServerResponse) -> RS<Vec<u8>> {
    let payload = serde_json::to_vec(response)
        .map_err(|e| m_error!(EC::EncodeErr, "encode server response error", e))?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

pub fn decode_server_response(frame: &Frame) -> RS<ServerResponse> {
    serde_json::from_slice(frame.payload())
        .map_err(|e| m_error!(EC::DecodeErr, "decode server response error", e))
}

pub fn encode_get_request(request_id: u64, request: &GetRequest) -> RS<Vec<u8>> {
    let payload = serde_json::to_vec(request)
        .map_err(|e| m_error!(EC::EncodeErr, "encode get request error", e))?;
    Ok(Frame::new(MessageType::Get, request_id, payload).encode())
}

pub fn decode_get_request(frame: &Frame) -> RS<GetRequest> {
    serde_json::from_slice(frame.payload())
        .map_err(|e| m_error!(EC::DecodeErr, "decode get request error", e))
}

pub fn decode_get_response(frame: &Frame) -> RS<GetResponse> {
    serde_json::from_slice(frame.payload())
        .map_err(|e| m_error!(EC::DecodeErr, "decode get response error", e))
}

pub fn encode_put_request(request_id: u64, request: &PutRequest) -> RS<Vec<u8>> {
    let payload = serde_json::to_vec(request)
        .map_err(|e| m_error!(EC::EncodeErr, "encode put request error", e))?;
    Ok(Frame::new(MessageType::Put, request_id, payload).encode())
}

pub fn decode_put_request(frame: &Frame) -> RS<PutRequest> {
    serde_json::from_slice(frame.payload())
        .map_err(|e| m_error!(EC::DecodeErr, "decode put request error", e))
}

pub fn decode_put_response(frame: &Frame) -> RS<PutResponse> {
    serde_json::from_slice(frame.payload())
        .map_err(|e| m_error!(EC::DecodeErr, "decode put response error", e))
}

pub fn encode_range_scan_request(request_id: u64, request: &RangeScanRequest) -> RS<Vec<u8>> {
    let payload = serde_json::to_vec(request)
        .map_err(|e| m_error!(EC::EncodeErr, "encode range scan request error", e))?;
    Ok(Frame::new(MessageType::RangeScan, request_id, payload).encode())
}

pub fn decode_range_scan_request(frame: &Frame) -> RS<RangeScanRequest> {
    serde_json::from_slice(frame.payload())
        .map_err(|e| m_error!(EC::DecodeErr, "decode range scan request error", e))
}

pub fn encode_procedure_invoke_request(
    request_id: u64,
    request: &ProcedureInvokeRequest,
) -> RS<Vec<u8>> {
    let payload = serde_json::to_vec(request)
        .map_err(|e| m_error!(EC::EncodeErr, "encode procedure invoke request error", e))?;
    Ok(Frame::new(MessageType::ProcedureInvoke, request_id, payload).encode())
}

pub fn encode_session_create_request(
    request_id: u64,
    request: &SessionCreateRequest,
) -> RS<Vec<u8>> {
    let payload = serde_json::to_vec(request)
        .map_err(|e| m_error!(EC::EncodeErr, "encode session create request error", e))?;
    Ok(Frame::new(MessageType::SessionCreate, request_id, payload).encode())
}

pub fn encode_session_close_request(request_id: u64, request: &SessionCloseRequest) -> RS<Vec<u8>> {
    let payload = serde_json::to_vec(request)
        .map_err(|e| m_error!(EC::EncodeErr, "encode session close request error", e))?;
    Ok(Frame::new(MessageType::SessionClose, request_id, payload).encode())
}

pub fn decode_range_scan_response(frame: &Frame) -> RS<RangeScanResponse> {
    serde_json::from_slice(frame.payload())
        .map_err(|e| m_error!(EC::DecodeErr, "decode range scan response error", e))
}

pub fn decode_procedure_invoke_request(frame: &Frame) -> RS<ProcedureInvokeRequest> {
    serde_json::from_slice(frame.payload())
        .map_err(|e| m_error!(EC::DecodeErr, "decode procedure invoke request error", e))
}

pub fn decode_session_create_response(frame: &Frame) -> RS<SessionCreateResponse> {
    serde_json::from_slice(frame.payload())
        .map_err(|e| m_error!(EC::DecodeErr, "decode session create response error", e))
}

pub fn decode_session_create_request(frame: &Frame) -> RS<SessionCreateRequest> {
    if frame.payload().is_empty() {
        return Ok(SessionCreateRequest::default());
    }
    serde_json::from_slice(frame.payload())
        .map_err(|e| m_error!(EC::DecodeErr, "decode session create request error", e))
}

pub fn decode_session_close_request(frame: &Frame) -> RS<SessionCloseRequest> {
    serde_json::from_slice(frame.payload())
        .map_err(|e| m_error!(EC::DecodeErr, "decode session close request error", e))
}

pub fn encode_get_response(request_id: u64, response: &GetResponse) -> RS<Vec<u8>> {
    let payload = serde_json::to_vec(response)
        .map_err(|e| m_error!(EC::EncodeErr, "encode get response error", e))?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

pub fn encode_put_response(request_id: u64, response: &PutResponse) -> RS<Vec<u8>> {
    let payload = serde_json::to_vec(response)
        .map_err(|e| m_error!(EC::EncodeErr, "encode put response error", e))?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

pub fn encode_range_scan_response(request_id: u64, response: &RangeScanResponse) -> RS<Vec<u8>> {
    let payload = serde_json::to_vec(response)
        .map_err(|e| m_error!(EC::EncodeErr, "encode range scan response error", e))?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

pub fn encode_procedure_invoke_response(
    request_id: u64,
    response: &ProcedureInvokeResponse,
) -> RS<Vec<u8>> {
    let payload = serde_json::to_vec(response)
        .map_err(|e| m_error!(EC::EncodeErr, "encode procedure invoke response error", e))?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

pub fn encode_session_create_response(
    request_id: u64,
    response: &SessionCreateResponse,
) -> RS<Vec<u8>> {
    let payload = serde_json::to_vec(response)
        .map_err(|e| m_error!(EC::EncodeErr, "encode session create response error", e))?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

pub fn encode_session_close_response(
    request_id: u64,
    response: &SessionCloseResponse,
) -> RS<Vec<u8>> {
    let payload = serde_json::to_vec(response)
        .map_err(|e| m_error!(EC::EncodeErr, "encode session close response error", e))?;
    Ok(Frame::new(MessageType::Response, request_id, payload).encode())
}

pub fn decode_procedure_invoke_response(frame: &Frame) -> RS<ProcedureInvokeResponse> {
    serde_json::from_slice(frame.payload())
        .map_err(|e| m_error!(EC::DecodeErr, "decode procedure invoke response error", e))
}

pub fn decode_session_close_response(frame: &Frame) -> RS<SessionCloseResponse> {
    serde_json::from_slice(frame.payload())
        .map_err(|e| m_error!(EC::DecodeErr, "decode session close response error", e))
}

pub fn encode_error_response(request_id: u64, message: impl Into<String>) -> RS<Vec<u8>> {
    let payload = serde_json::to_vec(&ErrorResponse::new(message))
        .map_err(|e| m_error!(EC::EncodeErr, "encode error response error", e))?;
    Ok(Frame::new(MessageType::Error, request_id, payload).encode())
}

pub fn decode_error_response(frame: &Frame) -> RS<ErrorResponse> {
    serde_json::from_slice(frame.payload())
        .map_err(|e| m_error!(EC::DecodeErr, "decode error response error", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip_preserves_header_and_payload() {
        let frame = Frame::new(MessageType::ProcedureInvoke, 42, b"payload".to_vec());
        let encoded = frame.encode();
        let decoded = Frame::decode(&encoded).unwrap();

        assert_eq!(decoded.header().magic(), 0x4d44);
        assert_eq!(decoded.header().version(), 1);
        assert_eq!(
            decoded.header().message_type(),
            MessageType::ProcedureInvoke
        );
        assert_eq!(decoded.header().request_id(), 42);
        assert_eq!(decoded.payload(), b"payload");
    }

    #[test]
    fn frame_decode_rejects_bad_magic_and_incomplete_payload() {
        let mut encoded = Frame::new(MessageType::Get, 7, vec![1, 2, 3]).encode();
        encoded[0] = 0;
        let bad_magic = Frame::decode(&encoded).unwrap_err();
        assert!(format!("{bad_magic}").contains("invalid frame magic"));

        let encoded = Frame::new(MessageType::Put, 8, vec![1, 2, 3, 4]).encode();
        let truncated = &encoded[..encoded.len() - 2];
        let incomplete = Frame::decode(truncated).unwrap_err();
        assert!(format!("{incomplete}").contains("frame payload is incomplete"));
    }

    #[test]
    fn query_and_execute_requests_roundtrip() {
        let request = ClientRequest::new("demo", "select 1");
        let query = encode_client_request(1, &request).unwrap();
        let query_frame = Frame::decode(&query).unwrap();
        assert_eq!(query_frame.header().message_type(), MessageType::Query);
        let query_decoded = decode_client_request(&query_frame).unwrap();
        assert_eq!(query_decoded.app_name(), "demo");
        assert_eq!(query_decoded.sql(), "select 1");

        let execute =
            encode_client_request_with_message_type(MessageType::Execute, 2, &request).unwrap();
        let execute_frame = Frame::decode(&execute).unwrap();
        assert_eq!(execute_frame.header().message_type(), MessageType::Execute);
        let execute_decoded = decode_client_request(&execute_frame).unwrap();
        assert_eq!(execute_decoded.app_name(), "demo");
        assert_eq!(execute_decoded.sql(), "select 1");
    }

    #[test]
    fn kv_and_session_messages_roundtrip() {
        let get_frame =
            Frame::decode(&encode_get_request(1, &GetRequest::new(9, b"key".to_vec())).unwrap())
                .unwrap();
        let get_request = decode_get_request(&get_frame).unwrap();
        assert_eq!(get_request.session_id(), 9);
        assert_eq!(get_request.key(), b"key");

        let put_frame = Frame::decode(
            &encode_put_request(2, &PutRequest::new(9, b"k".to_vec(), b"v".to_vec())).unwrap(),
        )
        .unwrap();
        let put_request = decode_put_request(&put_frame).unwrap();
        assert_eq!(put_request.session_id(), 9);
        assert_eq!(put_request.key(), b"k");
        assert_eq!(put_request.value(), b"v");
        assert_eq!(put_request.into_parts(), (b"k".to_vec(), b"v".to_vec()));

        let range_frame = Frame::decode(
            &encode_range_scan_request(3, &RangeScanRequest::new(9, b"a".to_vec(), b"z".to_vec()))
                .unwrap(),
        )
        .unwrap();
        let range_request = decode_range_scan_request(&range_frame).unwrap();
        assert_eq!(range_request.start_key(), b"a");
        assert_eq!(range_request.end_key(), b"z");

        let create_frame = Frame::decode(
            &encode_session_create_request(
                4,
                &SessionCreateRequest::new(Some("{\"partition\":1}".to_string())),
            )
            .unwrap(),
        )
        .unwrap();
        let create_request = decode_session_create_request(&create_frame).unwrap();
        assert_eq!(create_request.config_json(), Some("{\"partition\":1}"));

        let empty_create_frame = Frame::new(MessageType::SessionCreate, 5, vec![]);
        let empty_create_request = decode_session_create_request(&empty_create_frame).unwrap();
        assert_eq!(empty_create_request.config_json(), None);

        let close_frame =
            Frame::decode(&encode_session_close_request(6, &SessionCloseRequest::new(9)).unwrap())
                .unwrap();
        let close_request = decode_session_close_request(&close_frame).unwrap();
        assert_eq!(close_request.session_id(), 9);
    }

    #[test]
    fn invoke_and_response_messages_roundtrip() {
        let invoke_frame = Frame::decode(
            &encode_procedure_invoke_request(
                10,
                &ProcedureInvokeRequest::new(11, "app/mod/proc", b"input".to_vec()),
            )
            .unwrap(),
        )
        .unwrap();
        let invoke_request = decode_procedure_invoke_request(&invoke_frame).unwrap();
        assert_eq!(invoke_request.session_id(), 11);
        assert_eq!(invoke_request.procedure_name(), "app/mod/proc");
        assert_eq!(invoke_request.procedure_parameters(), b"input");
        assert_eq!(
            invoke_request.procedure_parameters_owned(),
            b"input".to_vec()
        );

        let response = ServerResponse::new(
            vec!["value".to_string()],
            vec![vec!["1".to_string()]],
            0,
            None,
        );
        let response_frame =
            Frame::decode(&encode_server_response(12, &response).unwrap()).unwrap();
        let decoded_response = decode_server_response(&response_frame).unwrap();
        assert_eq!(decoded_response.columns(), &["value".to_string()]);
        assert_eq!(decoded_response.rows(), &[vec!["1".to_string()]]);

        let get_response_frame = Frame::decode(
            &encode_get_response(13, &GetResponse::new(Some(b"v".to_vec()))).unwrap(),
        )
        .unwrap();
        assert_eq!(
            decode_get_response(&get_response_frame)
                .unwrap()
                .into_value(),
            Some(b"v".to_vec())
        );

        let range_response_frame = Frame::decode(
            &encode_range_scan_response(
                14,
                &RangeScanResponse::new(vec![KeyValue::new(b"k".to_vec(), b"v".to_vec())]),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            decode_range_scan_response(&range_response_frame)
                .unwrap()
                .into_items(),
            vec![KeyValue::new(b"k".to_vec(), b"v".to_vec())]
        );

        let invoke_response_frame = Frame::decode(
            &encode_procedure_invoke_response(15, &ProcedureInvokeResponse::new(b"ok".to_vec()))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            decode_procedure_invoke_response(&invoke_response_frame)
                .unwrap()
                .into_result(),
            b"ok".to_vec()
        );
    }

    #[test]
    fn error_response_roundtrip() {
        let frame = Frame::decode(&encode_error_response(99, "boom").unwrap()).unwrap();
        assert_eq!(frame.header().message_type(), MessageType::Error);
        let error = decode_error_response(&frame).unwrap();
        assert_eq!(error.message(), "boom");
    }
}
