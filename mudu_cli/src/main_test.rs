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
use mudu_binding::procedure::procedure_invoke;
use mudu_binding::universal::{uni_data_value::UniDataValue, uni_scalar_value::UniScalarValue};
use mudu_cli::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_cli::client::json_client::JsonClient;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use mudu_contract::protocol::{
    ClientRequest, GetRequest, GetResponse, KeyValue, ProcedureInvokeRequest,
    ProcedureInvokeResponse, PutRequest, PutResponse, RangeScanRequest, RangeScanResponse,
    ServerResponse, SessionCloseRequest, SessionCloseResponse, SessionCreateRequest,
    SessionCreateResponse,
};
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::type_family::TypeFamily;
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};

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
        let (mut socket, _) = listener.accept().unwrap();

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
    })
    .unwrap();

    addr
}

// ---------------------------------------------------------------------------
// run / command dispatch
// ---------------------------------------------------------------------------

#[test]
fn run_command_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let client = MockAsyncClient::new().with_command_response(select_1_response());
        let command = Commands::Command(json_args(r#"{"app_name":"demo","sql":"select 1"}"#));
        run_with_connectors(
            cli(command),
            &MockJsonConnector { client },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[test]
fn run_put_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let async_client = MockAsyncClient::new().with_put_response(PutResponse::new(true));
        let command = Commands::Put(json_args(r#"{"key":"k","value":"v"}"#));
        run_with_connectors(
            cli(command),
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: async_client,
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[test]
fn run_get_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let async_client = MockAsyncClient::new()
            .with_get_response(GetResponse::new(Some(json_string_value_bytes("value-1"))));
        let command = Commands::Get(json_args(r#"{"key":"k"}"#));
        run_with_connectors(
            cli(command),
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: async_client,
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[test]
fn run_range_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let async_client =
            MockAsyncClient::new().with_range_response(RangeScanResponse::new(vec![
                KeyValue::new(json_string_value_bytes("a"), json_string_value_bytes("1")),
            ]));
        let command = Commands::Range(json_args(r#"{"start_key":"a","end_key":"z"}"#));
        run_with_connectors(
            cli(command),
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: async_client,
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[test]
fn run_invoke_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let async_client = MockAsyncClient::new().with_invoke_procedure_response(
            ProcedureInvokeResponse::new(br#"{"ok":true}"#.to_vec()),
        );
        let command = Commands::Invoke(json_args(
            r#"{"procedure_name":"app/mod/proc","procedure_parameters":{}}"#,
        ));
        run_with_connectors(
            cli(command),
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: async_client,
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[test]
fn run_command_connect_failure_is_reported() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let command = Commands::Command(json_args(r#"{"app_name":"demo","sql":"select 1"}"#));
        let err = run_with_connectors(
            cli(command),
            &FailingJsonConnector,
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Network);
    })
    .unwrap();
}

// Starts a mock HTTP server; Miri's networking support is not reliable enough
// for this test, so run it natively.
#[cfg_attr(miri, ignore)]
#[test]
fn run_app_invoke_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(json!({
            "ok": true,
            "data": {
                "proc_desc": {
                    "module_name": "wallet",
                    "proc_name": "balance",
                    "param_desc": {"fields": []},
                    "return_desc": {"fields": []},
                }
            }
        }));

        let result = procedure_invoke::serialize_result(Ok(ProcedureResult::new(vec![]))).unwrap();
        let async_client = MockAsyncClient::new()
            .with_invoke_procedure_response(ProcedureInvokeResponse::new(result));

        let mut c = cli(Commands::AppInvoke(AppInvokeArgs {
            app: "wallet".to_string(),
            module: "wallet".to_string(),
            proc: "balance".to_string(),
            request: json_args("{}"),
        }));
        c.http_addr = http_addr;

        run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: async_client,
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_app_install_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(json!({"ok": true, "data": null}));
        let path = temp_json_path("app_install.mpk");
        mudu_sys::fs::sync::sync_write(&path, b"fake mpk").unwrap();

        let mut c = cli(Commands::AppInstall(AppInstallArgs { mpk: path.clone() }));
        c.http_addr = http_addr;

        run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap();

        let _ = mudu_sys::fs::sync::sync_remove_file(path);
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_app_list_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(json!({
            "ok": true,
            "data": [{"name": "wallet"}]
        }));
        let mut c = cli(Commands::AppList);
        c.http_addr = http_addr;

        run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_app_detail_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(json!({
            "ok": true,
            "data": {"app_name": "wallet"}
        }));
        let mut c = cli(Commands::AppDetail(AppDetailArgs {
            app: "wallet".to_string(),
            module: None,
            proc: None,
        }));
        c.http_addr = http_addr;

        run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_app_uninstall_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(json!({"ok": true, "data": null}));
        let mut c = cli(Commands::AppUninstall(AppUninstallArgs {
            app: "wallet".to_string(),
        }));
        c.http_addr = http_addr;

        run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_server_topology_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(json!({
            "ok": true,
            "data": {
                "worker_count": 1,
                "tcp_multi_port": false,
                "tcp_base_listen_port": 0,
                "workers": [
                    {
                        "worker_index": 0,
                        "tcp_listen_port": 9527,
                        "worker_id": {"h": 0, "l": 1},
                        "partitions": []
                    }
                ]
            }
        }));
        let mut c = cli(Commands::ServerTopology);
        c.http_addr = http_addr;

        run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_partition_route_with_key_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(json!({
            "ok": true,
            "data": {
                "routes": [
                    {
                        "partition_id": {"h": 0, "l": 1},
                        "worker_id": {"h": 0, "l": 2},
                    }
                ]
            }
        }));
        let mut c = cli(Commands::PartitionRoute(PartitionRouteArgs {
            rule_name: "user_rule".to_string(),
            key: Some(vec!["user-1".to_string()]),
            start: None,
            end: None,
        }));
        c.http_addr = http_addr;

        run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[test]
fn run_partition_route_rejects_key_and_range_together() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let command = Commands::PartitionRoute(PartitionRouteArgs {
            rule_name: "user_rule".to_string(),
            key: Some(vec!["user-1".to_string()]),
            start: Some(vec!["a".to_string()]),
            end: None,
        });
        let err = run_with_connectors(
            cli(command),
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    })
    .unwrap();
}

#[test]
fn run_partition_route_rejects_missing_key_and_range() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let command = Commands::PartitionRoute(PartitionRouteArgs {
            rule_name: "user_rule".to_string(),
            key: None,
            start: None,
            end: None,
        });
        let err = run_with_connectors(
            cli(command),
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    })
    .unwrap();
}

#[test]
fn run_app_detail_rejects_proc_without_module() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let command = Commands::AppDetail(AppDetailArgs {
            app: "wallet".to_string(),
            module: None,
            proc: Some("transfer".to_string()),
        });
        let err = run_with_connectors(
            cli(command),
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// JSON loading helpers
// ---------------------------------------------------------------------------

#[test]
fn load_json_request_parses_inline_json() {
    let args = json_args(r#"{"ok":true}"#);
    let value = load_json_request(args).unwrap();
    assert_eq!(value, json!({"ok": true}));
}

#[test]
fn load_json_request_rejects_invalid_json() {
    let args = json_args("not json");
    let err = load_json_request(args).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn load_json_request_reads_file() {
    let path = temp_json_path("load_json");
    mudu_sys::fs::sync::sync_write(&path, r#"{"from":"file"}"#).unwrap();
    let args = json_file_args(path.clone());
    let value = load_json_request(args).unwrap();
    assert_eq!(value, json!({"from": "file"}));
    let _ = mudu_sys::fs::sync::sync_remove_file(path);
}

#[test]
fn load_required_text_prefers_inline_in_test_module() {
    let text = load_required_text(Some("inline".to_string()), None).unwrap();
    assert_eq!(text, "inline");
}

#[test]
fn load_required_text_rejects_both_inputs() {
    let err = load_required_text(Some("a".to_string()), Some(PathBuf::from("/tmp/x"))).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidArgument);
}

#[test]
fn read_to_string_reads_input() {
    let mut input = "hello stdin".as_bytes();
    let text = read_to_string(&mut input).unwrap();
    assert_eq!(text, "hello stdin");
}

#[test]
fn read_to_string_rejects_empty_input() {
    let mut input = "".as_bytes();
    let err = read_to_string(&mut input).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidArgument);
}

#[test]
fn read_text_path_reads_dash_as_stdin_is_untested_but_file_path_works() {
    let path = temp_json_path("read_text_path");
    mudu_sys::fs::sync::sync_write(&path, "file contents").unwrap();
    let text = read_text_path(&path).unwrap();
    assert_eq!(text, "file contents");
    let _ = mudu_sys::fs::sync::sync_remove_file(path);
}

// ---------------------------------------------------------------------------
// Request body helpers
// ---------------------------------------------------------------------------

#[test]
fn with_oid_rejects_non_object_request() {
    let err = with_oid(json!("not an object"), 7).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidArgument);
}

#[test]
fn with_invoke_session_id_rejects_non_object_request() {
    let err = with_invoke_session_id(json!(42), 7).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidArgument);
}

#[test]
fn with_oid_handles_large_session_id() {
    let session_id = ((1u128) << 64) + 123;
    let request = with_oid(json!({"k":"v"}), session_id).unwrap();
    assert_eq!(request["oid"]["h"], json!(1u64));
    assert_eq!(request["oid"]["l"], json!(123u64));
}

// ---------------------------------------------------------------------------
// Output formatting
// ---------------------------------------------------------------------------

#[test]
fn print_json_to_writer_emits_pretty_json_by_default() {
    let mut buf = Vec::new();
    print_json_to_writer(&json!({"a": 1}), false, &mut buf).unwrap();
    let text = String::from_utf8(buf).unwrap();
    assert!(text.contains("{\n"));
    assert!(text.contains("\"a\": 1"));
}

#[test]
fn print_json_to_writer_emits_compact_json_when_requested() {
    let mut buf = Vec::new();
    print_json_to_writer(&json!({"a": 1}), true, &mut buf).unwrap();
    let text = String::from_utf8(buf).unwrap();
    assert!(text.contains("{\"a\":1}"));
}

#[test]
fn print_output_to_writer_respects_no_table() {
    let mut buf = Vec::new();
    let value = json!({"columns": ["c"], "rows": [["v"]], "affected_rows": 0, "error": null});
    print_output_to_writer(&value, false, false, true, &mut buf).unwrap();
    let text = String::from_utf8(buf).unwrap();
    assert!(text.contains("columns"));
}

#[test]
fn print_output_to_writer_table_flag_requires_tty() {
    // This test verifies the non-TTY path where `--table` is rejected because
    // a real terminal is required. When stdout/stdin are already terminals the
    // code path is different (it attempts to enter the TUI), so skip there.
    if io::stdout().is_terminal() || io::stdin().is_terminal() {
        return;
    }
    let mut buf = Vec::new();
    let value = json!({"columns": ["c"], "rows": [["v"]], "affected_rows": 0, "error": null});
    // In a non-TTY test environment the table branch reaches `run_query_table`,
    // which reports that a real TTY is required.
    let err = print_output_to_writer(&value, false, true, false, &mut buf).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidArgument);
}

// ---------------------------------------------------------------------------
// Shell helpers and run_shell
// ---------------------------------------------------------------------------

struct VecLineReader {
    lines: Vec<String>,
    index: usize,
}

impl VecLineReader {
    fn new(lines: Vec<String>) -> Self {
        Self { lines, index: 0 }
    }
}

impl ShellLineReader for VecLineReader {
    fn read_line(&mut self, _prompt: &str) -> Result<String, ShellReadError> {
        if self.index < self.lines.len() {
            let line = self.lines[self.index].clone();
            self.index += 1;
            Ok(line)
        } else {
            Err(ShellReadError::Eof)
        }
    }

    fn add_history(&mut self, _line: &str) {}
    fn load_history(&mut self, _path: &Path) {}
    fn save_history(&mut self, _path: &Path) {}
    fn clear_history(&mut self) {}
}

#[test]
fn handle_shell_meta_quits() {
    let mut app = "demo".to_string();
    assert!(handle_shell_meta("\\q", &mut app, &mut Vec::new()).unwrap());
    assert!(handle_shell_meta("\\exit", &mut app, &mut Vec::new()).unwrap());
    assert!(!handle_shell_meta("\\help", &mut app, &mut Vec::new()).unwrap());
}

#[test]
fn handle_shell_meta_switches_app() {
    let mut app = "demo".to_string();
    assert!(!handle_shell_meta("\\app kv", &mut app, &mut Vec::new()).unwrap());
    assert_eq!(app, "kv");
}

#[test]
fn handle_shell_meta_unknown_command_prints_help() {
    let mut app = "demo".to_string();
    assert!(!handle_shell_meta("\\foo", &mut app, &mut Vec::new()).unwrap());
}

#[test]
fn looks_like_query_detects_query_keywords() {
    assert!(looks_like_query("SELECT 1"));
    assert!(looks_like_query("  with t as (select 1) select * from t"));
    assert!(looks_like_query("show tables"));
    assert!(looks_like_query("DESCRIBE t"));
    assert!(!looks_like_query("INSERT INTO t VALUES (1)"));
    assert!(!looks_like_query(""));
}

#[test]
fn statement_complete_and_finalize_statement() {
    assert!(statement_complete("SELECT 1;"));
    assert!(statement_complete("SELECT 1;  "));
    assert!(!statement_complete("SELECT 1"));
    assert_eq!(finalize_statement("  SELECT 1;  "), "SELECT 1;");
}

#[test]
fn get_history_path_uses_home_or_userprofile() {
    // The result depends on environment variables; just ensure it returns a
    // path ending with the expected file name when one of the variables is set.
    if mudu_sys::env_var::var("HOME").is_some() || mudu_sys::env_var::var("USERPROFILE").is_some() {
        let path = get_history_path("demo").unwrap();
        assert!(path.to_string_lossy().contains(".mcli_history_demo"));
    }
}

#[test]
fn run_shell_executes_query_and_meta_commands() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let client = MockAsyncClient::new().with_command_response(select_1_response());
        let mut reader = VecLineReader::new(vec![
            "select 1;".to_string(),
            "\\app kv".to_string(),
            "\\q".to_string(),
        ]);
        let mut output = Vec::new();
        run_shell(
            "127.0.0.1:9527",
            ShellOutputOptions {
                compact: false,
                table: false,
                no_table: true, // force JSON output so the test is independent of TTY state
            },
            ShellArgs {
                app: "demo".to_string(),
            },
            &MockJsonConnector { client },
            &mut reader,
            &mut output,
        )
        .await
        .unwrap();

        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("Enter SQL terminated by"));
        assert!(text.contains("\"rows\""));
        assert!(text.contains("app = kv"));
    })
    .unwrap();
}

#[test]
fn run_shell_supports_multi_line_statements() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let client = MockAsyncClient::new().with_command_response(select_1_response());
        let mut reader = VecLineReader::new(vec![
            "select".to_string(),
            " 1;".to_string(),
            "\\q".to_string(),
        ]);
        let mut output = Vec::new();
        run_shell(
            "127.0.0.1:9527",
            ShellOutputOptions {
                compact: false,
                table: false,
                no_table: true, // force JSON output so the test is independent of TTY state
            },
            ShellArgs {
                app: "demo".to_string(),
            },
            &MockJsonConnector { client },
            &mut reader,
            &mut output,
        )
        .await
        .unwrap();

        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("\"rows\""));
    })
    .unwrap();
}

#[test]
fn run_shell_executes_non_query_as_execute() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let client = MockAsyncClient::new().with_command_response(affected_rows_response(3));
        let mut reader = VecLineReader::new(vec![
            "insert into t values (1);".to_string(),
            "\\q".to_string(),
        ]);
        let mut output = Vec::new();
        run_shell(
            "127.0.0.1:9527",
            ShellOutputOptions {
                compact: false,
                table: false,
                no_table: true, // force JSON output so the test is independent of TTY state
            },
            ShellArgs {
                app: "demo".to_string(),
            },
            &MockJsonConnector { client },
            &mut reader,
            &mut output,
        )
        .await
        .unwrap();

        let text = String::from_utf8(output).unwrap();
        assert!(text.contains("affected_rows: 3"));
    })
    .unwrap();
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
