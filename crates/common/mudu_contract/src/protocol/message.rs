//! Client request and server response message types.

use crate::tuple::tuple_field_desc::TupleFieldDesc;
use crate::tuple::tuple_value::TupleValue;
use mudu_type::data_value::DataValue;
use serde::{Deserialize, Serialize};

/// Client handshake request advertising supported protocol versions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandshakeRequest {
    /// Protocol frame versions supported by the client.
    pub supported_versions: Vec<u32>,
    /// Optional client capability tags.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// Server handshake response carrying the negotiated protocol version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandshakeResponse {
    /// Negotiated protocol frame version.
    pub selected_version: u32,
    /// Optional server capability tags.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// Client SQL request carrying the application name, statement text, and parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRequest {
    oid: u128,
    app_name: String,
    sql: String,
    /// SQL parameters for the `?` placeholders in `sql`, in placeholder order.
    /// NUMERIC values travel in their plain string form; the server coerces
    /// them to the target NUMERIC type during bind.
    #[serde(default)]
    params: Vec<DataValue>,
}

/// Server response to a query or execute request.
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
    /// Trace id the digest belongs to.
    pub trace_id: u64,
    /// Per-stage durations in nanoseconds, indexed by `TxnStage`.
    pub durations_ns: [Option<u64>; mudu_sys_contract::perf::TxnStage::Count as usize],
}

impl ServerPerfDigest {
    /// Creates a digest for `trace_id` with no stage durations recorded.
    pub fn new(trace_id: u64) -> Self {
        Self {
            trace_id,
            ..Default::default()
        }
    }

    /// Records the duration in nanoseconds of a transaction stage.
    pub fn set(&mut self, stage: mudu_sys_contract::perf::TxnStage, ns: u64) {
        self.durations_ns[stage.as_index()] = Some(ns);
    }

    /// Returns the recorded duration of a transaction stage, if any.
    pub fn get(&self, stage: mudu_sys_contract::perf::TxnStage) -> Option<u64> {
        self.durations_ns[stage.as_index()]
    }
}

/// Key-value get request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRequest {
    session_id: u128,
    key: Vec<u8>,
}

/// Key-value put request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutRequest {
    session_id: u128,
    key: Vec<u8>,
    value: Vec<u8>,
}

/// Key-value range scan request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeScanRequest {
    session_id: u128,
    start_key: Vec<u8>,
    end_key: Vec<u8>,
}

/// Procedure invocation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcedureInvokeRequest {
    session_id: u128,
    procedure_name: String,
    procedure_parameters: Vec<u8>,
}

/// Session creation request.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionCreateRequest {
    config_json: Option<String>,
}

/// Session creation response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreateResponse {
    session_id: u128,
}

/// Session close request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCloseRequest {
    session_id: u128,
}

/// Session close response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionCloseResponse {
    closed: bool,
}

/// A key-value pair.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyValue {
    key: Vec<u8>,
    value: Vec<u8>,
}

/// Key-value get response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetResponse {
    value: Option<Vec<u8>>,
}

/// Key-value put response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PutResponse {
    ok: bool,
}

/// Key-value range scan response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RangeScanResponse {
    items: Vec<KeyValue>,
}

/// Procedure invocation response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProcedureInvokeResponse {
    result: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    server_perf_digest: Option<ServerPerfDigest>,
}

impl ClientRequest {
    /// Creates a request with a zero object id and no parameters.
    pub fn new(app_name: impl Into<String>, sql: impl Into<String>) -> Self {
        Self {
            oid: 0,
            app_name: app_name.into(),
            sql: sql.into(),
            params: Vec::new(),
        }
    }

    /// Creates a request with the given object id and no parameters.
    pub fn new_with_oid(oid: u128, app_name: impl Into<String>, sql: impl Into<String>) -> Self {
        Self {
            oid,
            app_name: app_name.into(),
            sql: sql.into(),
            params: Vec::new(),
        }
    }

    /// Sets the SQL parameters of the request.
    pub fn with_params(mut self, params: Vec<DataValue>) -> Self {
        self.params = params;
        self
    }

    /// Returns the session/object id of the request.
    pub fn oid(&self) -> u128 {
        self.oid
    }

    /// Returns the application name.
    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    /// Returns the SQL statement text.
    pub fn sql(&self) -> &str {
        &self.sql
    }

    /// Returns the SQL parameters.
    pub fn params(&self) -> &[DataValue] {
        &self.params
    }
}

impl ServerResponse {
    /// Creates a response with rows, affected-row count, and an optional error message.
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

    /// Attaches a server-side performance digest to the response.
    pub fn with_server_perf_digest(mut self, digest: ServerPerfDigest) -> Self {
        self.server_perf_digest = Some(digest);
        self
    }

    /// Returns the server-side performance digest, if attached.
    pub fn server_perf_digest(&self) -> Option<&ServerPerfDigest> {
        self.server_perf_digest.as_ref()
    }

    /// Returns the description of the result row fields.
    pub fn row_desc(&self) -> &TupleFieldDesc {
        &self.row_desc
    }

    /// Returns the result rows.
    pub fn rows(&self) -> &[TupleValue] {
        &self.rows
    }

    /// Returns the number of affected rows.
    pub fn affected_rows(&self) -> u64 {
        self.affected_rows
    }

    /// Returns the error message, if the request failed.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

impl GetRequest {
    /// Creates a get request for `key` in session `session_id`.
    pub fn new(session_id: u128, key: Vec<u8>) -> Self {
        Self { session_id, key }
    }

    /// Returns the session id.
    pub fn session_id(&self) -> u128 {
        self.session_id
    }

    /// Returns the key to fetch.
    pub fn key(&self) -> &[u8] {
        &self.key
    }
}

impl PutRequest {
    /// Creates a put request storing `value` at `key` in session `session_id`.
    pub fn new(session_id: u128, key: Vec<u8>, value: Vec<u8>) -> Self {
        Self {
            session_id,
            key,
            value,
        }
    }

    /// Returns the session id.
    pub fn session_id(&self) -> u128 {
        self.session_id
    }

    /// Returns the key to store at.
    pub fn key(&self) -> &[u8] {
        &self.key
    }

    /// Returns the value to store.
    pub fn value(&self) -> &[u8] {
        &self.value
    }

    /// Consumes the request and returns the key and value.
    pub fn into_parts(self) -> (Vec<u8>, Vec<u8>) {
        (self.key, self.value)
    }
}

impl RangeScanRequest {
    /// Creates a range scan over `start_key..end_key` in session `session_id`.
    pub fn new(session_id: u128, start_key: Vec<u8>, end_key: Vec<u8>) -> Self {
        Self {
            session_id,
            start_key,
            end_key,
        }
    }

    /// Returns the session id.
    pub fn session_id(&self) -> u128 {
        self.session_id
    }

    /// Returns the inclusive start key of the scan.
    pub fn start_key(&self) -> &[u8] {
        &self.start_key
    }

    /// Returns the end key of the scan.
    pub fn end_key(&self) -> &[u8] {
        &self.end_key
    }
}

impl ProcedureInvokeRequest {
    /// Creates an invocation of `procedure_name` with serialized `procedure_parameters`.
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

    /// Returns the session id.
    pub fn session_id(&self) -> u128 {
        self.session_id
    }

    /// Returns the name of the procedure to invoke.
    pub fn procedure_name(&self) -> &str {
        &self.procedure_name
    }

    /// Returns the serialized procedure parameters.
    pub fn procedure_parameters(&self) -> &[u8] {
        &self.procedure_parameters
    }

    /// Returns a copy of the serialized procedure parameters.
    pub fn procedure_parameters_owned(&self) -> Vec<u8> {
        self.procedure_parameters.clone()
    }
}

impl SessionCreateRequest {
    /// Creates a session creation request with an optional JSON configuration.
    pub fn new(config_json: Option<String>) -> Self {
        Self { config_json }
    }

    /// Returns the optional JSON session configuration.
    pub fn config_json(&self) -> Option<&str> {
        self.config_json.as_deref()
    }
}

impl KeyValue {
    /// Creates a key-value pair.
    pub fn new(key: Vec<u8>, value: Vec<u8>) -> Self {
        Self { key, value }
    }

    /// Returns the key.
    pub fn key(&self) -> &[u8] {
        &self.key
    }

    /// Returns the value.
    pub fn value(&self) -> &[u8] {
        &self.value
    }
}

impl GetResponse {
    /// Creates a get response with an optional value.
    pub fn new(value: Option<Vec<u8>>) -> Self {
        Self { value }
    }

    /// Returns the fetched value, if the key exists.
    pub fn value(&self) -> Option<&[u8]> {
        self.value.as_deref()
    }

    /// Consumes the response and returns the fetched value, if any.
    pub fn into_value(self) -> Option<Vec<u8>> {
        self.value
    }
}

impl PutResponse {
    /// Creates a put response.
    pub fn new(ok: bool) -> Self {
        Self { ok }
    }

    /// Returns whether the put succeeded.
    pub fn ok(&self) -> bool {
        self.ok
    }
}

impl RangeScanResponse {
    /// Creates a range scan response from the scanned items.
    pub fn new(items: Vec<KeyValue>) -> Self {
        Self { items }
    }

    /// Returns the scanned key-value pairs.
    pub fn items(&self) -> &[KeyValue] {
        &self.items
    }

    /// Consumes the response and returns the scanned key-value pairs.
    pub fn into_items(self) -> Vec<KeyValue> {
        self.items
    }
}

impl ProcedureInvokeResponse {
    /// Creates an invocation response from serialized result bytes.
    pub fn new(result: Vec<u8>) -> Self {
        Self {
            result,
            server_perf_digest: None,
        }
    }

    /// Returns the serialized procedure result.
    pub fn result(&self) -> &[u8] {
        &self.result
    }

    /// Consumes the response and returns the serialized procedure result.
    pub fn into_result(self) -> Vec<u8> {
        self.result
    }

    /// Attaches a server-side performance digest to the response.
    pub fn with_server_perf_digest(mut self, digest: ServerPerfDigest) -> Self {
        self.server_perf_digest = Some(digest);
        self
    }

    /// Returns the server-side performance digest, if attached.
    pub fn server_perf_digest(&self) -> Option<&ServerPerfDigest> {
        self.server_perf_digest.as_ref()
    }
}

impl SessionCreateResponse {
    /// Creates a session creation response carrying the new session id.
    pub fn new(session_id: u128) -> Self {
        Self { session_id }
    }

    /// Returns the id of the newly created session.
    pub fn session_id(&self) -> u128 {
        self.session_id
    }
}

impl SessionCloseRequest {
    /// Creates a session close request for `session_id`.
    pub fn new(session_id: u128) -> Self {
        Self { session_id }
    }

    /// Returns the id of the session to close.
    pub fn session_id(&self) -> u128 {
        self.session_id
    }
}

impl SessionCloseResponse {
    /// Creates a session close response.
    pub fn new(closed: bool) -> Self {
        Self { closed }
    }

    /// Returns whether the session was closed.
    pub fn closed(&self) -> bool {
        self.closed
    }
}
