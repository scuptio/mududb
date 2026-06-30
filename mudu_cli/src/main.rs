#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! `mcli` — command-line client for MuduDB.
//!
//! Reads command-line arguments, sends TCP or HTTP requests to a MuduDB
//! server, and prints the response as JSON or an interactive table.

use clap::{Args, Parser, Subcommand};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::procedure::procedure_invoke;
use mudu_cli::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_cli::client::json_client::JsonClient;
use mudu_cli::management::{
    fetch_app_detail, fetch_app_list, fetch_proc_desc, fetch_server_topology, install_app_package,
    route_partition, uninstall_app,
};
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::protocol::{ProcedureInvokeRequest, SessionCloseRequest, SessionCreateRequest};
use mudu_contract::tuple::datum_desc::DatumDesc;
use serde_json::{Value, json};
use std::io::IsTerminal;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use async_trait::async_trait;

const CLI_EXAMPLES: &str = "\
Examples:
  mcli --addr 127.0.0.1:9527 command --json '{\"app_name\":\"demo\",\"sql\":\"select 1\"}'
  mcli --addr 127.0.0.1:9527 shell --app demo
  mcli --addr 127.0.0.1:9527 put --json-file put.json
  cat invoke.json | mcli --addr 127.0.0.1:9527 invoke --json-file -
  mcli --http-addr 127.0.0.1:8300 app-install --mpk target/wasm32-wasip2/release/key-value.mpk
  mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke --app kv --module key_value --proc kv_read --json '{\"user_key\":\"user-1\"}'
  mcli --http-addr 127.0.0.1:8300 app-list
  mcli --http-addr 127.0.0.1:8300 app-detail --app wallet
  mcli --http-addr 127.0.0.1:8300 app-uninstall --app wallet
  mcli --http-addr 127.0.0.1:8300 server-topology
  mcli --http-addr 127.0.0.1:8300 partition-route --rule-name user_rule --key user-100";

/// Top-level command-line arguments for `mcli`.
#[derive(Parser, Debug)]
#[command(name = "mcli")]
#[command(version)]
#[command(about = "TCP protocol client for MuduDB")]
#[command(after_help = CLI_EXAMPLES)]
struct Cli {
    #[arg(long, global = true, default_value = "127.0.0.1:9527")]
    addr: String,
    #[arg(long, global = true, default_value = "127.0.0.1:8300")]
    http_addr: String,
    #[arg(
        long,
        global = true,
        help = "Render SQL query results as an interactive table (ratatui). Auto-enabled on TTY."
    )]
    table: bool,
    #[arg(
        long,
        global = true,
        conflicts_with = "table",
        visible_alias = "no-tui",
        help = "Disable ratatui TUI rendering and always print JSON."
    )]
    no_table: bool,
    #[arg(
        long,
        global = true,
        help = "Print compact JSON instead of pretty JSON."
    )]
    compact: bool,
    #[command(subcommand)]
    command: Commands,
}

/// Available `mcli` subcommands.
#[derive(Subcommand, Debug)]
enum Commands {
    /// Send a SQL query or execute request encoded as JSON.
    Command(JsonRequestArgs),
    /// Interactive SQL shell (like mysql/psql).
    Shell(ShellArgs),
    /// Put a key-value item using a JSON request body.
    Put(JsonRequestArgs),
    /// Get a key using a JSON request body.
    Get(JsonRequestArgs),
    /// Scan a key range using a JSON request body.
    Range(JsonRequestArgs),
    /// Invoke a procedure using a JSON request body.
    Invoke(JsonRequestArgs),
    /// Install a .mpk package through the HTTP management API.
    AppInstall(AppInstallArgs),
    /// Invoke an installed procedure through the TCP protocol.
    AppInvoke(AppInvokeArgs),
    /// List installed apps via HTTP management API.
    AppList,
    /// Show app procedures or one procedure detail via HTTP management API.
    AppDetail(AppDetailArgs),
    /// Uninstall an app via HTTP management API.
    AppUninstall(AppUninstallArgs),
    /// Get worker topology via HTTP management API.
    ServerTopology,
    /// Route a partition key/range via HTTP management API.
    PartitionRoute(PartitionRouteArgs),
}

/// Arguments for subcommands that take an inline JSON body or a JSON file.
#[derive(Args, Debug)]
struct JsonRequestArgs {
    #[arg(long, conflicts_with = "json_file", help = "Inline JSON request body.")]
    json: Option<String>,
    #[arg(
        long = "json-file",
        conflicts_with = "json",
        help = "Read JSON request body from a file. Use '-' to read from stdin."
    )]
    json_file: Option<PathBuf>,
}

/// Arguments for the `app-install` subcommand.
#[derive(Args, Debug)]
struct AppInstallArgs {
    #[arg(long, help = "Path to the .mpk package file to install.")]
    mpk: PathBuf,
}

/// Arguments for the `app-invoke` subcommand.
#[derive(Args, Debug)]
struct AppInvokeArgs {
    #[arg(long)]
    app: String,
    #[arg(long)]
    module: String,
    #[arg(long)]
    proc: String,
    #[command(flatten)]
    request: JsonRequestArgs,
}

/// Arguments for the `shell` subcommand.
#[derive(Args, Debug)]
struct ShellArgs {
    #[arg(
        long,
        default_value = "demo",
        help = "Initial app name to run queries against."
    )]
    app: String,
}

/// Arguments for the `app-detail` subcommand.
#[derive(Args, Debug)]
struct AppDetailArgs {
    #[arg(long)]
    app: String,
    #[arg(long)]
    module: Option<String>,
    #[arg(long)]
    proc: Option<String>,
}

/// Arguments for the `app-uninstall` subcommand.
#[derive(Args, Debug)]
struct AppUninstallArgs {
    #[arg(long)]
    app: String,
}

/// Arguments for the `partition-route` subcommand.
#[derive(Args, Debug)]
struct PartitionRouteArgs {
    #[arg(long = "rule-name")]
    rule_name: String,
    #[arg(long, value_delimiter = ',')]
    key: Option<Vec<String>>,
    #[arg(long, value_delimiter = ',')]
    start: Option<Vec<String>>,
    #[arg(long, value_delimiter = ',')]
    end: Option<Vec<String>>,
}

/// Trait for connecting a [`JsonClient`] during command dispatch.
#[async_trait]
pub(crate) trait JsonClientConnect: Send + Sync {
    /// Async client type produced by this connector.
    type Client: AsyncClient;
    /// Connect to `addr` and return a JSON client backed by the async client.
    async fn connect(&self, addr: &str) -> RS<JsonClient<Self::Client>>;
}

/// Trait for connecting a raw async TCP client during command dispatch.
#[async_trait]
pub(crate) trait AsyncClientConnect: Send + Sync {
    /// Async client type produced by this connector.
    type Client: AsyncClient;
    /// Connect to `addr` and return an async client.
    async fn connect(&self, addr: &str) -> RS<Self::Client>;
}

struct RealJsonConnector;

#[async_trait]
impl JsonClientConnect for RealJsonConnector {
    type Client = AsyncClientImpl;

    async fn connect(&self, addr: &str) -> RS<JsonClient<Self::Client>> {
        JsonClient::connect(addr).await
    }
}

struct RealAsyncConnector;

#[async_trait]
impl AsyncClientConnect for RealAsyncConnector {
    type Client = AsyncClientImpl;

    async fn connect(&self, addr: &str) -> RS<Self::Client> {
        AsyncClientImpl::connect(addr).await
    }
}

fn main() {
    let cli = Cli::parse();
    mudu_sys::task::async_::block_on_async_current(async {
        if let Err(err) = run(cli).await {
            eprintln!("{err}");
            mudu_sys::process::exit(1);
        }
    });
}

async fn run(cli: Cli) -> RS<()> {
    let compact = cli.compact;
    let table = cli.table;
    let no_table = cli.no_table;
    let output = run_with_connectors(cli, &RealJsonConnector, &RealAsyncConnector).await?;
    print_output(&output, compact, table, no_table)?;
    Ok(())
}

async fn run_with_connectors<JC, AC>(
    cli: Cli,
    json_connector: &JC,
    async_connector: &AC,
) -> RS<Value>
where
    JC: JsonClientConnect,
    AC: AsyncClientConnect,
{
    let Cli {
        addr,
        http_addr,
        compact,
        table,
        no_table,
        command,
    } = cli;

    let output = match command {
        Commands::Command(args) => {
            let request = load_json_request(args)?;
            let mut client = json_connector.connect(&addr).await.map_err(|e| {
                mudu_error!(
                    ErrorCode::Network,
                    format!("connect {} failed: {}", addr, e)
                )
            })?;
            client.command(request).await.map_err(|e| {
                mudu_error!(ErrorCode::Network, format!("command request failed: {}", e))
            })?
        }
        Commands::Shell(args) => {
            let mut rl = rustyline::DefaultEditor::new()
                .map_err(|e| mudu_error!(ErrorCode::Io, format!("init readline failed: {e}")))?;
            let mut stdout = io::stdout();
            run_shell(
                &addr,
                ShellOutputOptions {
                    compact,
                    table,
                    no_table,
                },
                args,
                json_connector,
                &mut rl,
                &mut stdout,
            )
            .await?;
            return Ok(json!({ "status": "ok" }));
        }
        Commands::Put(args) => {
            let request = load_json_request(args)?;
            let mut client = async_connector.connect(&addr).await.map_err(|e| {
                mudu_error!(
                    ErrorCode::Network,
                    format!("connect {} failed: {}", addr, e)
                )
            })?;
            let session_id = client
                .create_session(SessionCreateRequest::new(None))
                .await
                .map_err(|e| {
                    mudu_error!(
                        ErrorCode::Network,
                        format!("session-create for put failed: {}", e)
                    )
                })?
                .session_id();
            let request = with_oid(request, session_id)?;
            let mut client = JsonClient::new(client);
            let response = client.put(request).await.map_err(|e| {
                mudu_error!(ErrorCode::Network, format!("put request failed: {}", e))
            })?;
            let _ = client
                .into_inner()
                .close_session(SessionCloseRequest::new(session_id))
                .await;
            response
        }
        Commands::Get(args) => {
            let request = load_json_request(args)?;
            let mut client = async_connector.connect(&addr).await.map_err(|e| {
                mudu_error!(
                    ErrorCode::Network,
                    format!("connect {} failed: {}", addr, e)
                )
            })?;
            let session_id = client
                .create_session(SessionCreateRequest::new(None))
                .await
                .map_err(|e| {
                    mudu_error!(
                        ErrorCode::Network,
                        format!("session-create for get failed: {}", e)
                    )
                })?
                .session_id();
            let request = with_oid(request, session_id)?;
            let mut client = JsonClient::new(client);
            let response = client.get(request).await.map_err(|e| {
                mudu_error!(ErrorCode::Network, format!("get request failed: {}", e))
            })?;
            let _ = client
                .into_inner()
                .close_session(SessionCloseRequest::new(session_id))
                .await;
            response
        }
        Commands::Range(args) => {
            let request = load_json_request(args)?;
            let mut client = async_connector.connect(&addr).await.map_err(|e| {
                mudu_error!(
                    ErrorCode::Network,
                    format!("connect {} failed: {}", addr, e)
                )
            })?;
            let session_id = client
                .create_session(SessionCreateRequest::new(None))
                .await
                .map_err(|e| {
                    mudu_error!(
                        ErrorCode::Network,
                        format!("session-create for range failed: {}", e)
                    )
                })?
                .session_id();
            let request = with_oid(request, session_id)?;
            let mut client = JsonClient::new(client);
            let response = client.range(request).await.map_err(|e| {
                mudu_error!(ErrorCode::Network, format!("range request failed: {}", e))
            })?;
            let _ = client
                .into_inner()
                .close_session(SessionCloseRequest::new(session_id))
                .await;
            response
        }
        Commands::Invoke(args) => {
            let request = load_json_request(args)?;
            let mut client = async_connector.connect(&addr).await.map_err(|e| {
                mudu_error!(
                    ErrorCode::Network,
                    format!("connect {} failed: {}", addr, e)
                )
            })?;
            let session_id = client
                .create_session(SessionCreateRequest::new(None))
                .await
                .map_err(|e| {
                    mudu_error!(
                        ErrorCode::Network,
                        format!("session-create for invoke failed: {}", e)
                    )
                })?
                .session_id();
            let request = with_invoke_session_id(request, session_id)?;
            let mut client = JsonClient::new(client);
            let response = client.invoke(request).await.map_err(|e| {
                mudu_error!(ErrorCode::Network, format!("invoke request failed: {}", e))
            })?;
            let _ = client
                .into_inner()
                .close_session(SessionCloseRequest::new(session_id))
                .await;
            response
        }
        Commands::AppInstall(args) => {
            let mpk_binary = mudu_sys::fs::sync::sync_read_all(&args.mpk).map_err(|e| {
                mudu_error!(
                    ErrorCode::Io,
                    format!("read {} failed: {}", args.mpk.display(), e)
                )
            })?;
            install_app_package(&http_addr, mpk_binary)
                .await
                .map_err(|e| mudu_error!(ErrorCode::Network, e))?;
            let mut response = json!({
                "status": "ok",
            });
            if let Value::Object(ref mut map) = response {
                map.insert(
                    "mpk_path".to_string(),
                    Value::String(args.mpk.display().to_string()),
                );
            }
            response
        }
        Commands::AppInvoke(args) => {
            let request = load_json_request(args.request)?;
            let proc_desc = fetch_proc_desc(&http_addr, &args.app, &args.module, &args.proc)
                .await
                .map_err(|e| mudu_error!(ErrorCode::Network, e))?;
            let request_object = request.as_object().cloned().ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidArgument,
                    "invoke request JSON must be an object"
                )
            })?;
            let param = to_param(&request_object, proc_desc.param_desc().fields())?;
            let payload = procedure_invoke::serialize_param(param).map_err(|e| {
                mudu_error!(
                    ErrorCode::Encode,
                    format!("serialize procedure param failed: {}", e)
                )
            })?;
            let mut client = async_connector.connect(&addr).await.map_err(|e| {
                mudu_error!(
                    ErrorCode::Network,
                    format!("connect {} failed: {}", addr, e)
                )
            })?;
            let session_id = client
                .create_session(SessionCreateRequest::new(None))
                .await
                .map_err(|e| {
                    mudu_error!(
                        ErrorCode::Network,
                        format!("session-create for app-invoke failed: {}", e)
                    )
                })?
                .session_id();
            let result_binary = client
                .invoke_procedure(ProcedureInvokeRequest::new(
                    session_id,
                    format!("{}/{}/{}", args.app, args.module, args.proc),
                    payload,
                ))
                .await
                .map_err(|e| mudu_error!(ErrorCode::Network, format!("tcp invoke failed: {}", e)))?
                .into_result();
            let _ = client
                .close_session(SessionCloseRequest::new(session_id))
                .await;
            let result = procedure_invoke::deserialize_result(&result_binary).map_err(|e| {
                mudu_error!(
                    ErrorCode::Decode,
                    format!("deserialize procedure result failed: {}", e)
                )
            })?;
            procedure_invoke::result_to_json(result).map_err(|e| {
                mudu_error!(
                    ErrorCode::Decode,
                    format!("convert procedure result to JSON failed: {}", e)
                )
            })?
        }
        Commands::AppList => fetch_app_list(&http_addr)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, e))?,
        Commands::AppDetail(args) => {
            if args.proc.is_some() && args.module.is_none() {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    "--proc requires --module"
                ));
            }
            fetch_app_detail(
                &http_addr,
                &args.app,
                args.module.as_deref(),
                args.proc.as_deref(),
            )
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, e))?
        }
        Commands::AppUninstall(args) => {
            uninstall_app(&http_addr, &args.app)
                .await
                .map_err(|e| mudu_error!(ErrorCode::Network, e))?;
            json!({
                "status": "ok",
                "app": args.app,
            })
        }
        Commands::ServerTopology => serde_json::to_value(
            fetch_server_topology(&http_addr)
                .await
                .map_err(|e| mudu_error!(ErrorCode::Network, e))?,
        )
        .map_err(|e| {
            mudu_error!(
                ErrorCode::Encode,
                format!("serialize server topology failed: {}", e)
            )
        })?,
        Commands::PartitionRoute(args) => {
            if args.key.is_some() && (args.start.is_some() || args.end.is_some()) {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    "use either --key or (--start/--end), not both"
                ));
            }
            if args.key.is_none() && args.start.is_none() && args.end.is_none() {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    "partition-route requires either --key or at least one of --start/--end"
                ));
            }
            serde_json::to_value(
                route_partition(&http_addr, &args.rule_name, args.key, args.start, args.end)
                    .await
                    .map_err(|e| mudu_error!(ErrorCode::Network, e))?,
            )
            .map_err(|e| {
                mudu_error!(
                    ErrorCode::Encode,
                    format!("serialize partition route response failed: {}", e)
                )
            })?
        }
    };

    Ok(output)
}

fn load_json_request(args: JsonRequestArgs) -> RS<Value> {
    let raw = load_required_text(args.json, args.json_file)?;
    serde_json::from_str(&raw)
        .map_err(|e| mudu_error!(ErrorCode::Decode, format!("invalid JSON request: {}", e)))
}

fn load_required_text(inline: Option<String>, file: Option<PathBuf>) -> RS<String> {
    match (inline, file) {
        (Some(text), None) => read_special_text_input(text),
        (None, Some(path)) => read_text_path(&path),
        (None, None) => Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "either --json or --json-file is required"
        )),
        (Some(_), Some(_)) => Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "use either inline text or file input, not both"
        )),
    }
}

fn with_oid(request: Value, session_id: u128) -> RS<Value> {
    let mut request = request
        .as_object()
        .cloned()
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "request JSON must be an object"))?;
    request.insert(
        "oid".to_string(),
        json!({
            "h": (session_id >> 64) as u64,
            "l": session_id as u64,
        }),
    );
    Ok(Value::Object(request))
}

fn with_invoke_session_id(request: Value, session_id: u128) -> RS<Value> {
    let mut request = request
        .as_object()
        .cloned()
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "request JSON must be an object"))?;
    request.insert("session_id".to_string(), json!(session_id.to_string()));
    Ok(Value::Object(request))
}

fn read_special_text_input(text: String) -> RS<String> {
    if text == "-" {
        read_stdin_to_string()
    } else {
        Ok(text)
    }
}

fn read_text_path(path: &PathBuf) -> RS<String> {
    if path.as_os_str() == "-" {
        read_stdin_to_string()
    } else {
        mudu_sys::fs::sync::sync_read_to_string(path)
    }
}

fn read_stdin_to_string() -> RS<String> {
    read_to_string(&mut io::stdin())
}

fn read_to_string(reader: &mut impl Read) -> RS<String> {
    let mut buf = String::new();
    reader
        .read_to_string(&mut buf)
        .map_err(|e| mudu_error!(ErrorCode::Io, format!("read stdin failed: {}", e)))?;
    if buf.is_empty() {
        return Err(mudu_error!(ErrorCode::InvalidArgument, "stdin is empty"));
    }
    Ok(buf)
}

fn print_json_to_writer(value: &Value, compact: bool, writer: &mut impl Write) -> RS<()> {
    let rendered = if compact {
        serde_json::to_string(value)
    } else {
        serde_json::to_string_pretty(value)
    }
    .map_err(|e| mudu_error!(ErrorCode::Encode, format!("serialize output failed: {}", e)))?;
    writeln!(writer, "{rendered}")
        .map_err(|e| mudu_error!(ErrorCode::Io, format!("write output failed: {}", e)))?;
    Ok(())
}

fn print_output(value: &Value, compact: bool, table: bool, no_table: bool) -> RS<()> {
    print_output_to_writer(value, compact, table, no_table, &mut io::stdout())
}

fn print_output_to_writer(
    value: &Value,
    compact: bool,
    table: bool,
    no_table: bool,
    writer: &mut impl Write,
) -> RS<()> {
    let interactive_tty = io::stdout().is_terminal() && io::stdin().is_terminal();
    if !compact
        && !no_table
        && (table || interactive_tty)
        && let Some(table) = mudu_cli::tui::extract_query_table(value)
    {
        mudu_cli::tui::run_query_table(table)
            .map_err(|e| mudu_error!(ErrorCode::InvalidArgument, e))?;
        return Ok(());
    }
    print_json_to_writer(value, compact, writer)
}

/// Error returned by a shell line reader.
#[derive(Debug)]
enum ShellReadError {
    /// User interrupted input (e.g. Ctrl-C).
    Interrupted,
    /// End of input stream.
    Eof,
    /// Other I/O error.
    Other(String),
}

/// Abstraction over `rustyline::DefaultEditor` so the shell can be unit-tested
/// without a real terminal.
trait ShellLineReader {
    /// Read one line using `prompt`.
    fn read_line(&mut self, prompt: &str) -> Result<String, ShellReadError>;
    /// Add `line` to the in-memory history.
    fn add_history(&mut self, line: &str);
    /// Load history from `path`.
    fn load_history(&mut self, path: &Path);
    /// Save history to `path`.
    fn save_history(&mut self, path: &Path);
    /// Clear the in-memory history.
    fn clear_history(&mut self);
}

impl ShellLineReader for rustyline::DefaultEditor {
    fn read_line(&mut self, prompt: &str) -> Result<String, ShellReadError> {
        match self.readline(prompt) {
            Ok(line) => Ok(line),
            Err(rustyline::error::ReadlineError::Interrupted) => Err(ShellReadError::Interrupted),
            Err(rustyline::error::ReadlineError::Eof) => Err(ShellReadError::Eof),
            Err(e) => Err(ShellReadError::Other(format!("{e}"))),
        }
    }

    fn add_history(&mut self, line: &str) {
        let _ = self.add_history_entry(line);
    }

    fn load_history(&mut self, path: &Path) {
        let _ = self.load_history(path);
    }

    fn save_history(&mut self, path: &Path) {
        let _ = self.save_history(path);
    }

    fn clear_history(&mut self) {
        let _ = self.clear_history();
    }
}

/// Output formatting options passed to [`run_shell`].
#[derive(Clone, Copy)]
struct ShellOutputOptions {
    /// Print compact JSON instead of pretty JSON.
    compact: bool,
    /// Render SQL query results as an interactive table.
    table: bool,
    /// Disable ratatui TUI rendering and always print JSON.
    no_table: bool,
}

async fn run_shell<JC, R, W>(
    addr: &str,
    options: ShellOutputOptions,
    args: ShellArgs,
    connector: &JC,
    reader: &mut R,
    output: &mut W,
) -> RS<()>
where
    JC: JsonClientConnect,
    R: ShellLineReader,
    W: Write,
{
    let mut app = args.app;
    let mut client = connector.connect(addr).await.map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            format!("connect {} failed: {}", addr, e)
        )
    })?;

    if let Some(path) = get_history_path(&app) {
        reader.load_history(&path);
    }

    let mut buffer = String::new();

    writeln!(
        output,
        "Enter SQL terminated by ';'. Meta commands: \\q, \\help, \\app <name>."
    )
    .map_err(|e| mudu_error!(ErrorCode::Io, format!("write output failed: {e}")))?;

    loop {
        let prompt = if buffer.trim().is_empty() {
            format!("mudu({app})> ")
        } else {
            "....> ".to_string()
        };

        let line = match reader.read_line(&prompt) {
            Ok(line) => line,
            Err(ShellReadError::Interrupted) => {
                buffer.clear();
                writeln!(output, "^C")
                    .map_err(|e| mudu_error!(ErrorCode::Io, format!("write output failed: {e}")))?;
                continue;
            }
            Err(ShellReadError::Eof) => break,
            Err(ShellReadError::Other(e)) => {
                return Err(mudu_error!(ErrorCode::Io, format!("readline failed: {e}")));
            }
        };

        let trimmed = line.trim();
        if buffer.is_empty() && trimmed.starts_with('\\') {
            let old_app = app.clone();
            if handle_shell_meta(trimmed, &mut app, output)? {
                break;
            }
            if old_app != app {
                if let Some(path) = get_history_path(&old_app) {
                    reader.save_history(&path);
                }
                reader.clear_history();
                if let Some(path) = get_history_path(&app) {
                    reader.load_history(&path);
                }
            }
            continue;
        }

        if trimmed.is_empty() && buffer.is_empty() {
            continue;
        }

        buffer.push_str(&line);
        buffer.push('\n');

        if !statement_complete(&buffer) {
            continue;
        }

        let statement = finalize_statement(&buffer);
        buffer.clear();
        if statement.is_empty() {
            continue;
        }

        reader.add_history(statement.as_str());
        if let Some(path) = get_history_path(&app) {
            reader.save_history(&path);
        }

        let is_query = looks_like_query(&statement);
        let request = if is_query {
            json!({ "app_name": app, "sql": statement })
        } else {
            json!({ "app_name": app, "sql": statement, "kind": "execute" })
        };

        let response = client
            .command(request)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, format!("request failed: {e}")))?;

        if response
            .get("error")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        {
            print_json_to_writer(&response, options.compact, output)?;
            continue;
        }

        if let Some(table_value) = mudu_cli::tui::extract_query_table(&response) {
            let interactive_tty = io::stdout().is_terminal() && io::stdin().is_terminal();
            if !options.compact && !options.no_table && (options.table || interactive_tty) {
                mudu_cli::tui::run_query_table(table_value)
                    .map_err(|e| mudu_error!(ErrorCode::InvalidArgument, e))?;
            } else {
                print_json_to_writer(&response, options.compact, output)?;
            }
            continue;
        }

        let affected = response
            .get("affected_rows")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        writeln!(output, "affected_rows: {affected}")
            .map_err(|e| mudu_error!(ErrorCode::Io, format!("write output failed: {e}")))?;
    }

    Ok(())
}

fn get_history_path(app: &str) -> Option<PathBuf> {
    mudu_sys::env_var::var("HOME")
        .or(mudu_sys::env_var::var("USERPROFILE"))
        .map(|h| {
            let mut path = std::path::PathBuf::from(h);
            path.push(format!(".mcli_history_{}", app));
            path
        })
}

fn handle_shell_meta(input: &str, app: &mut String, output: &mut impl Write) -> RS<bool> {
    let mut parts = input.split_whitespace();
    let cmd = parts.next().unwrap_or("");
    match cmd {
        "\\q" | "\\quit" | "\\exit" => Ok(true),
        "\\help" | "\\h" => {
            writeln!(output, "Meta commands:")
                .map_err(|e| mudu_error!(ErrorCode::Io, format!("write output failed: {e}")))?;
            writeln!(output, "  \\q                 quit")
                .map_err(|e| mudu_error!(ErrorCode::Io, format!("write output failed: {e}")))?;
            writeln!(output, "  \\app <name>        switch app")
                .map_err(|e| mudu_error!(ErrorCode::Io, format!("write output failed: {e}")))?;
            writeln!(output, "  \\help              show this help")
                .map_err(|e| mudu_error!(ErrorCode::Io, format!("write output failed: {e}")))?;
            writeln!(output, "SQL:")
                .map_err(|e| mudu_error!(ErrorCode::Io, format!("write output failed: {e}")))?;
            writeln!(output, "  End statements with ';' (multi-line supported).")
                .map_err(|e| mudu_error!(ErrorCode::Io, format!("write output failed: {e}")))?;
            Ok(false)
        }
        "\\app" => {
            if let Some(name) = parts.next() {
                *app = name.to_string();
                writeln!(output, "app = {app}")
                    .map_err(|e| mudu_error!(ErrorCode::Io, format!("write output failed: {e}")))?;
            } else {
                writeln!(output, "usage: \\app <name>")
                    .map_err(|e| mudu_error!(ErrorCode::Io, format!("write output failed: {e}")))?;
            }
            Ok(false)
        }
        _ => {
            writeln!(output, "unknown meta command: {cmd} (try \\help)")
                .map_err(|e| mudu_error!(ErrorCode::Io, format!("write output failed: {e}")))?;
            Ok(false)
        }
    }
}

fn statement_complete(buf: &str) -> bool {
    buf.trim_end().ends_with(';')
}

fn finalize_statement(buf: &str) -> String {
    buf.trim().to_string()
}

fn looks_like_query(sql: &str) -> bool {
    let first = sql
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        first.as_str(),
        "select" | "with" | "show" | "describe" | "desc" | "pragma" | "explain"
    )
}

fn to_param(argv: &serde_json::Map<String, Value>, desc: &[DatumDesc]) -> RS<ProcedureParam> {
    let mut vec = vec![];
    for datum_desc in desc {
        let value = argv.get(datum_desc.name()).cloned().ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidArgument,
                format!("missing parameter {}", datum_desc.name())
            )
        })?;
        let dat_value = datum_desc.dat_type_id().fn_input_json()(&value, datum_desc.dat_type())
            .map_err(|e| {
                mudu_error!(
                    ErrorCode::Decode,
                    format!("convert parameter {} failed: {}", datum_desc.name(), e)
                )
            })?;
        vec.push(dat_value);
    }
    Ok(ProcedureParam::new(0, 0, vec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mudu_contract::procedure::proc_desc::ProcDesc;
    use std::time::UNIX_EPOCH;

    #[test]
    fn load_required_text_prefers_inline() {
        let text = load_required_text(Some("{\"ok\":true}".to_string()), None).unwrap();
        assert_eq!(text, "{\"ok\":true}");
    }

    #[test]
    fn load_required_text_reads_file() {
        let path = unique_temp_path("mudu_cli_json");
        mudu_sys::fs::sync::sync_write(&path, "{\"v\":1}").unwrap();
        let text = load_required_text(None, Some(path.clone())).unwrap();
        assert_eq!(text, "{\"v\":1}");
        mudu_sys::fs::sync::sync_remove_file(path).unwrap();
    }

    #[test]
    fn load_required_text_requires_input() {
        let err = load_required_text(None, None).unwrap_err();
        assert!(err.to_string().contains("--json"));
    }

    #[test]
    fn to_param_builds_procedure_param_from_json() {
        let proc_desc: ProcDesc = serde_json::from_value(json!({
            "module_name": "key_value",
            "proc_name": "kv_insert",
            "param_desc": {
                "fields": [
                    {
                        "name": "user_key",
                        "dat_type": {
                            "id": "String",
                            "param": {
                                "String": {
                                    "length": 65536
                                }
                            }
                        }
                    },
                    {
                        "name": "value",
                        "dat_type": {
                            "id": "String",
                            "param": {
                                "String": {
                                    "length": 65536
                                }
                            }
                        }
                    }
                ]
            },
            "return_desc": { "fields": [] }
        }))
        .unwrap();

        let argv = json!({
            "user_key": "user-1",
            "value": "value-1"
        })
        .as_object()
        .unwrap()
        .clone();
        let param = to_param(&argv, proc_desc.param_desc().fields()).unwrap();
        assert_eq!(param.param_list().len(), 2);
    }

    #[test]
    fn read_text_path_rejects_missing_file() {
        let path = PathBuf::from("/tmp/mcli_missing_input.json");
        let err = read_text_path(&path).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);
    }

    #[test]
    fn with_oid_injects_oid_value() {
        let request = with_oid(json!({"key": "user-1"}), 99).unwrap();
        assert_eq!(request["oid"], json!({"h": 0, "l": 99}));
        assert_eq!(request["key"], json!("user-1"));
    }

    #[test]
    fn with_invoke_session_id_injects_session_id_string() {
        let request =
            with_invoke_session_id(json!({"procedure_name": "app/mod/proc"}), 99).unwrap();
        assert_eq!(request["session_id"], json!("99"));
        assert_eq!(request["procedure_name"], json!("app/mod/proc"));
    }

    #[test]
    fn test_statement_complete() {
        assert!(statement_complete("SELECT 1;"));
        assert!(statement_complete("SELECT 1;  "));
        assert!(!statement_complete("SELECT 1"));
    }

    #[test]
    fn test_finalize_statement() {
        assert_eq!(finalize_statement("SELECT 1;"), "SELECT 1;");
        assert_eq!(finalize_statement("  SELECT 1;  "), "SELECT 1;");
        assert_eq!(finalize_statement("SELECT 1"), "SELECT 1");
    }

    fn unique_temp_path(prefix: &str) -> PathBuf {
        let nanos = mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        mudu_sys::env_var::temp_dir().join(format!("{prefix}_{nanos}.json"))
    }
}

#[cfg(test)]
mod main_test;
