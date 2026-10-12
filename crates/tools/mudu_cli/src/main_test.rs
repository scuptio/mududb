//! Tests for the `mcli` binary in `main.rs`.
//!
//! These tests exercise command dispatch, argument parsing, JSON loading,
//! output formatting and shell helpers without requiring a live MuduDB server.
//! TCP transports are mocked with [`MockAsyncClient`] and HTTP management
//! calls are served by a minimal local HTTP/1.1 server.

use super::*;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::{uni_data_value::UniDataValue, uni_scalar_value::UniScalarValue};
use mudu_client::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_client::client::json_client::JsonClient;
use mudu_contract::protocol::{
    ClientRequest, GetRequest, GetResponse, ProcedureInvokeRequest, ProcedureInvokeResponse,
    PutRequest, PutResponse, RangeScanRequest, RangeScanResponse, ServerResponse,
    SessionCloseRequest, SessionCloseResponse, SessionCreateRequest, SessionCreateResponse,
};
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::type_family::TypeFamily;
use serde_json::Value;
use std::io::{Read, Write};
use std::path::PathBuf;

mod io_output;
mod repl;
mod shell;
mod subcommands;

// ---------------------------------------------------------------------------
// Mock TCP client and connectors
// ---------------------------------------------------------------------------

/// A configurable async client used to fake the MuduDB TCP protocol.
#[derive(Clone)]
struct MockAsyncClient {
    command_response: Option<ServerResponse>,
    put_response: Option<PutResponse>,
    get_response: Option<GetResponse>,
    range_response: Option<RangeScanResponse>,
    invoke_procedure_response: Option<ProcedureInvokeResponse>,
    session_id: u128,
}

impl MockAsyncClient {
    fn new() -> Self {
        Self {
            command_response: None,
            put_response: None,
            get_response: None,
            range_response: None,
            invoke_procedure_response: None,
            session_id: 42,
        }
    }

    fn with_command_response(mut self, response: ServerResponse) -> Self {
        self.command_response = Some(response);
        self
    }

    fn with_put_response(mut self, response: PutResponse) -> Self {
        self.put_response = Some(response);
        self
    }

    fn with_get_response(mut self, response: GetResponse) -> Self {
        self.get_response = Some(response);
        self
    }

    fn with_range_response(mut self, response: RangeScanResponse) -> Self {
        self.range_response = Some(response);
        self
    }

    fn with_invoke_procedure_response(mut self, response: ProcedureInvokeResponse) -> Self {
        self.invoke_procedure_response = Some(response);
        self
    }
}

#[async_trait]
impl AsyncClient for MockAsyncClient {
    async fn query(&mut self, _request: ClientRequest) -> RS<ServerResponse> {
        self.command_response
            .clone()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "unexpected query"))
    }

    async fn execute(&mut self, _request: ClientRequest) -> RS<ServerResponse> {
        self.command_response
            .clone()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "unexpected execute"))
    }

    async fn batch(&mut self, _request: ClientRequest) -> RS<ServerResponse> {
        Err(mudu_error!(ErrorCode::Internal, "unexpected batch"))
    }

    async fn get(&mut self, _request: GetRequest) -> RS<GetResponse> {
        self.get_response
            .take()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "unexpected get"))
    }

    async fn put(&mut self, _request: PutRequest) -> RS<PutResponse> {
        self.put_response
            .take()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "unexpected put"))
    }

    async fn range_scan(&mut self, _request: RangeScanRequest) -> RS<RangeScanResponse> {
        self.range_response
            .take()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "unexpected range_scan"))
    }

    async fn invoke_procedure(
        &mut self,
        _request: ProcedureInvokeRequest,
    ) -> RS<ProcedureInvokeResponse> {
        self.invoke_procedure_response
            .take()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "unexpected invoke_procedure"))
    }

    async fn create_session(
        &mut self,
        _request: SessionCreateRequest,
    ) -> RS<SessionCreateResponse> {
        Ok(SessionCreateResponse::new(self.session_id))
    }

    async fn close_session(&mut self, _request: SessionCloseRequest) -> RS<SessionCloseResponse> {
        Ok(SessionCloseResponse::new(true))
    }
}

struct MockJsonConnector {
    client: MockAsyncClient,
}

#[async_trait]
impl JsonClientConnect for MockJsonConnector {
    type Client = MockAsyncClient;

    async fn connect(&self, _addr: &str) -> RS<JsonClient<Self::Client>> {
        Ok(JsonClient::new(self.client.clone()))
    }
}

struct MockAsyncConnector {
    client: MockAsyncClient,
}

#[async_trait]
impl AsyncClientConnect for MockAsyncConnector {
    type Client = MockAsyncClient;

    async fn connect(&self, _addr: &str) -> RS<Self::Client> {
        Ok(self.client.clone())
    }
}

struct FailingJsonConnector;

#[async_trait]
impl JsonClientConnect for FailingJsonConnector {
    type Client = AsyncClientImpl;

    async fn connect(&self, _addr: &str) -> RS<JsonClient<Self::Client>> {
        Err(mudu_error!(ErrorCode::Network, "mock connect failure"))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn select_1_response() -> ServerResponse {
    ServerResponse::new(
        TupleFieldDesc::new(vec![DatumDesc::new(
            "value".to_string(),
            DataType::default_for(TypeFamily::String),
        )]),
        vec![TupleValue::from(vec![DataValue::from_string(
            "1".to_string(),
        )])],
        0,
        None,
    )
}

fn affected_rows_response(rows: u64) -> ServerResponse {
    ServerResponse::new(TupleFieldDesc::new(vec![]), vec![], rows, None)
}

fn json_string_value_bytes(value: &str) -> Vec<u8> {
    serde_json::to_vec(&UniDataValue::from_scalar(UniScalarValue::from_string(
        value.to_string(),
    )))
    .unwrap()
}

fn cli(command: Commands) -> Cli {
    Cli {
        addr: "127.0.0.1:9527".to_string(),
        http_addr: "127.0.0.1:8300".to_string(),
        compact: false,
        table: false,
        no_table: false,
        command,
    }
}

fn json_args(json: &str) -> JsonRequestArgs {
    JsonRequestArgs {
        json: Some(json.to_string()),
        json_file: None,
    }
}

fn json_file_args(path: PathBuf) -> JsonRequestArgs {
    JsonRequestArgs {
        json: None,
        json_file: Some(path),
    }
}

// ---------------------------------------------------------------------------
// Mock HTTP server for management API tests
// ---------------------------------------------------------------------------

fn start_mock_http_server(response_body: Value) -> String {
    use mudu_sys::net::sync::bind_tcp;
    use std::net::SocketAddr;

    let listener = bind_tcp("127.0.0.1:0".parse::<SocketAddr>().unwrap()).unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let body = serde_json::to_string(&response_body).unwrap();

    mudu_sys::task::sync::spawn_thread_named("mock-http", move || {
        // Serve connections in a loop: the CLI retries failed requests, so the
        // mock must keep listening for more than one connection. Otherwise a
        // transient first-attempt failure (e.g. under heavy load) closes the
        // listener and every retry fails with "connection refused".
        loop {
            let (mut socket, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(_) => continue,
            };

            let mut header_buf = Vec::new();
            let mut byte = [0u8; 1];
            loop {
                if socket.read_exact(&mut byte).is_err() {
                    break;
                }
                header_buf.push(byte[0]);
                if header_buf.ends_with(b"\r\n\r\n") {
                    break;
                }
            }

            let headers = String::from_utf8_lossy(&header_buf);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length: ")
                        .or_else(|| line.strip_prefix("content-length: "))
                })
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(0);

            if content_length > 0 {
                let mut body_buf = vec![0u8; content_length];
                let _ = socket.read_exact(&mut body_buf);
            }

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes());
        }
    })
    .unwrap();

    addr
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn temp_json_path(prefix: &str) -> PathBuf {
    use std::time::UNIX_EPOCH;
    let nanos = mudu_sys::time::system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    mudu_sys::env_var::temp_dir().join(format!("{prefix}_{nanos}.json"))
}
