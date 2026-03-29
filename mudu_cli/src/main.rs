use base64::Engine;
use clap::{Args, Parser, Subcommand};
use mudu_binding::procedure::procedure_invoke;
use mudu_cli::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_cli::client::json_client::JsonClient;
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::protocol::{ProcedureInvokeRequest, SessionCloseRequest, SessionCreateRequest};
use mudu_contract::tuple::datum_desc::DatumDesc;
use serde_json::{Value, json};
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

type AppResult<T> = Result<T, String>;

const CLI_EXAMPLES: &str = "\
Examples:
  mcli --addr 127.0.0.1:9000 command --json '{\"app_name\":\"demo\",\"sql\":\"select 1\"}'
  mcli put --json-file put.json
  cat invoke.json | mcli invoke --json-file -
  mcli app-install --mpk target/wasm32-wasip2/release/key-value.mpk
  mcli app-invoke --app kv --module key_value --proc kv_read --json '{\"user_key\":\"user-1\"}'";

#[derive(Parser, Debug)]
#[command(name = "mcli")]
#[command(version)]
#[command(about = "TCP protocol client for MuduDB")]
#[command(after_help = CLI_EXAMPLES)]
struct Cli {
    #[arg(long, global = true, default_value = "127.0.0.1:9000")]
    addr: String,
    #[arg(long, global = true, default_value = "127.0.0.1:8300")]
    http_addr: String,
    #[arg(
        long,
        global = true,
        help = "Print compact JSON instead of pretty JSON."
    )]
    compact: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Send a SQL query or execute request encoded as JSON.
    Command(JsonRequestArgs),
    /// Put a key-value item using a JSON request body.
    Put(JsonRequestArgs),
    /// Get a key using a JSON request body.
    Get(JsonRequestArgs),
    /// Scan a key range using a JSON request body.
    Range(JsonRequestArgs),
    /// Invoke a procedure using a JSON request body.
    Invoke(JsonRequestArgs),
    /// Install a .mpk package through the management HTTP API.
    AppInstall(AppInstallArgs),
    /// Invoke an installed procedure through the TCP protocol.
    AppInvoke(AppInvokeArgs),
}

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

#[derive(Args, Debug)]
struct AppInstallArgs {
    #[arg(long, help = "Path to the .mpk package file to install.")]
    mpk: PathBuf,
}

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

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli).await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> AppResult<()> {
    let output = match cli.command {
        Commands::Command(args) => {
            let request = load_json_request(args)?;
            let mut client = JsonClient::connect(&cli.addr)
                .await
                .map_err(|e| format!("connect {} failed: {}", cli.addr, e))?;
            client
                .command(request)
                .await
                .map_err(|e| format!("command request failed: {}", e))?
        }
        Commands::Put(args) => {
            let request = load_json_request(args)?;
            let mut client = AsyncClientImpl::connect(&cli.addr)
                .await
                .map_err(|e| format!("connect {} failed: {}", cli.addr, e))?;
            let session_id = client
                .create_session(SessionCreateRequest::new(None))
                .await
                .map_err(|e| format!("session-create for put failed: {}", e))?
                .session_id();
            let request = with_session_id(request, session_id)?;
            let mut client = JsonClient::new(client);
            let response = client
                .put(request)
                .await
                .map_err(|e| format!("put request failed: {}", e))?;
            let _ = client
                .into_inner()
                .close_session(SessionCloseRequest::new(session_id))
                .await;
            response
        }
        Commands::Get(args) => {
            let request = load_json_request(args)?;
            let mut client = AsyncClientImpl::connect(&cli.addr)
                .await
                .map_err(|e| format!("connect {} failed: {}", cli.addr, e))?;
            let session_id = client
                .create_session(SessionCreateRequest::new(None))
                .await
                .map_err(|e| format!("session-create for get failed: {}", e))?
                .session_id();
            let request = with_session_id(request, session_id)?;
            let mut client = JsonClient::new(client);
            let response = client
                .get(request)
                .await
                .map_err(|e| format!("get request failed: {}", e))?;
            let _ = client
                .into_inner()
                .close_session(SessionCloseRequest::new(session_id))
                .await;
            response
        }
        Commands::Range(args) => {
            let request = load_json_request(args)?;
            let mut client = AsyncClientImpl::connect(&cli.addr)
                .await
                .map_err(|e| format!("connect {} failed: {}", cli.addr, e))?;
            let session_id = client
                .create_session(SessionCreateRequest::new(None))
                .await
                .map_err(|e| format!("session-create for range failed: {}", e))?
                .session_id();
            let request = with_session_id(request, session_id)?;
            let mut client = JsonClient::new(client);
            let response = client
                .range(request)
                .await
                .map_err(|e| format!("range request failed: {}", e))?;
            let _ = client
                .into_inner()
                .close_session(SessionCloseRequest::new(session_id))
                .await;
            response
        }
        Commands::Invoke(args) => {
            let request = load_json_request(args)?;
            let mut client = AsyncClientImpl::connect(&cli.addr)
                .await
                .map_err(|e| format!("connect {} failed: {}", cli.addr, e))?;
            let session_id = client
                .create_session(SessionCreateRequest::new(None))
                .await
                .map_err(|e| format!("session-create for invoke failed: {}", e))?
                .session_id();
            let request = with_session_id(request, session_id)?;
            let mut client = JsonClient::new(client);
            let response = client
                .invoke(request)
                .await
                .map_err(|e| format!("invoke request failed: {}", e))?;
            let _ = client
                .into_inner()
                .close_session(SessionCloseRequest::new(session_id))
                .await;
            response
        }
        Commands::AppInstall(args) => {
            let mpk_binary = fs::read(&args.mpk)
                .map_err(|e| format!("read {} failed: {}", args.mpk.display(), e))?;
            let payload = json!({
                "mpk_base64": base64::engine::general_purpose::STANDARD.encode(mpk_binary),
            });
            let response = post_http_json(&cli.http_addr, "/mudu/app/install", payload).await?;
            let _ = extract_http_api_data(response)?;
            json!({
                "installed": true,
                "mpk_path": args.mpk.display().to_string(),
            })
        }
        Commands::AppInvoke(args) => {
            let request = load_json_request(args.request)?;
            let proc_desc =
                fetch_proc_desc(&cli.http_addr, &args.app, &args.module, &args.proc).await?;
            let request_object = request
                .as_object()
                .cloned()
                .ok_or_else(|| "invoke request JSON must be an object".to_string())?;
            let param = to_param(&request_object, proc_desc.param_desc().fields())?;
            let payload = procedure_invoke::serialize_param(param)
                .map_err(|e| format!("serialize procedure param failed: {}", e))?;
            let mut client = AsyncClientImpl::connect(&cli.addr)
                .await
                .map_err(|e| format!("connect {} failed: {}", cli.addr, e))?;
            let session_id = client
                .create_session(SessionCreateRequest::new(None))
                .await
                .map_err(|e| format!("session-create for app-invoke failed: {}", e))?
                .session_id();
            let result_binary = client
                .invoke_procedure(ProcedureInvokeRequest::new(
                    session_id,
                    format!("{}/{}/{}", args.app, args.module, args.proc),
                    payload,
                ))
                .await
                .map_err(|e| format!("tcp invoke failed: {}", e))?
                .into_result();
            let _ = client
                .close_session(SessionCloseRequest::new(session_id))
                .await;
            let result = procedure_invoke::deserialize_result(&result_binary)
                .map_err(|e| format!("deserialize procedure result failed: {}", e))?;
            procedure_invoke::result_to_json(result)
                .map_err(|e| format!("convert procedure result to JSON failed: {}", e))?
        }
    };

    print_json(&output, cli.compact)?;
    Ok(())
}

fn load_json_request(args: JsonRequestArgs) -> AppResult<Value> {
    let raw = load_required_text(args.json, args.json_file)?;
    serde_json::from_str(&raw).map_err(|e| format!("invalid JSON request: {}", e))
}

fn load_required_text(inline: Option<String>, file: Option<PathBuf>) -> AppResult<String> {
    match (inline, file) {
        (Some(text), None) => read_special_text_input(text),
        (None, Some(path)) => read_text_path(&path),
        (None, None) => Err("either --json or --json-file is required".to_string()),
        (Some(_), Some(_)) => Err("use either inline text or file input, not both".to_string()),
    }
}

fn with_session_id(request: Value, session_id: u128) -> AppResult<Value> {
    let mut request = request
        .as_object()
        .cloned()
        .ok_or_else(|| "request JSON must be an object".to_string())?;
    request.insert(
        "oid".to_string(),
        json!({
            "h": (session_id >> 64) as u64,
            "l": session_id as u64,
        }),
    );
    Ok(Value::Object(request))
}

fn read_special_text_input(text: String) -> AppResult<String> {
    if text == "-" {
        read_stdin_to_string()
    } else {
        Ok(text)
    }
}

fn read_text_path(path: &PathBuf) -> AppResult<String> {
    if path.as_os_str() == "-" {
        read_stdin_to_string()
    } else {
        fs::read_to_string(path).map_err(|e| format!("read {} failed: {}", path.display(), e))
    }
}

fn read_stdin_to_string() -> AppResult<String> {
    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| format!("read stdin failed: {}", e))?;
    if buf.is_empty() {
        return Err("stdin is empty".to_string());
    }
    Ok(buf)
}

fn print_json(value: &Value, compact: bool) -> AppResult<()> {
    let rendered = if compact {
        serde_json::to_string(value)
    } else {
        serde_json::to_string_pretty(value)
    }
    .map_err(|e| format!("serialize output failed: {}", e))?;
    println!("{rendered}");
    Ok(())
}

async fn post_http_json(http_addr: &str, path: &str, payload: Value) -> AppResult<Value> {
    let url = format!("http://{}{}", http_addr, path);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("build HTTP client failed: {}", e))?;
    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("POST {} failed: {}", url, e))?;
    response
        .json::<Value>()
        .await
        .map_err(|e| format!("decode HTTP response from {} failed: {}", url, e))
}

async fn get_http_json(http_addr: &str, path: &str) -> AppResult<Value> {
    let url = format!("http://{}{}", http_addr, path);
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("build HTTP client failed: {}", e))?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("GET {} failed: {}", url, e))?;
    response
        .json::<Value>()
        .await
        .map_err(|e| format!("decode HTTP response from {} failed: {}", url, e))
}

fn extract_http_api_data(response: Value) -> AppResult<Value> {
    let status = response
        .get("status")
        .and_then(Value::as_i64)
        .ok_or_else(|| "HTTP API response missing numeric status".to_string())?;
    if status == 0 {
        return Ok(response.get("data").cloned().unwrap_or(Value::Null));
    }
    let message = response
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("HTTP API request failed");
    let data = response.get("data").cloned().unwrap_or(Value::Null);
    Err(format!("{}: {}", message, data))
}

async fn fetch_proc_desc(
    http_addr: &str,
    app: &str,
    module: &str,
    proc_name: &str,
) -> AppResult<ProcDesc> {
    let response = get_http_json(
        http_addr,
        &format!("/mudu/app/list/{}/{}/{}", app, module, proc_name),
    )
    .await?;
    let data = extract_http_api_data(response)?;
    let proc_desc = data
        .get("proc_desc")
        .cloned()
        .ok_or_else(|| "procedure detail response missing proc_desc".to_string())?;
    serde_json::from_value(proc_desc).map_err(|e| format!("decode proc_desc failed: {}", e))
}

fn to_param(
    argv: &serde_json::Map<String, Value>,
    desc: &[DatumDesc],
) -> AppResult<ProcedureParam> {
    let mut vec = vec![];
    for datum_desc in desc {
        let value = argv
            .get(datum_desc.name())
            .cloned()
            .ok_or_else(|| format!("missing parameter {}", datum_desc.name()))?;
        let dat_value = datum_desc.dat_type_id().fn_input_json()(&value, datum_desc.dat_type())
            .map_err(|e| format!("convert parameter {} failed: {}", datum_desc.name(), e))?;
        vec.push(dat_value);
    }
    Ok(ProcedureParam::new(0, 0, vec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn load_required_text_prefers_inline() {
        let text = load_required_text(Some("{\"ok\":true}".to_string()), None).unwrap();
        assert_eq!(text, "{\"ok\":true}");
    }

    #[test]
    fn load_required_text_reads_file() {
        let path = unique_temp_path("mudu_cli_json");
        fs::write(&path, "{\"v\":1}").unwrap();
        let text = load_required_text(None, Some(path.clone())).unwrap();
        assert_eq!(text, "{\"v\":1}");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn load_required_text_requires_input() {
        let err = load_required_text(None, None).unwrap_err();
        assert!(err.contains("--json"));
    }

    #[test]
    fn extract_http_api_data_returns_data_on_success() {
        let value = extract_http_api_data(json!({
            "status": 0,
            "message": "ok",
            "data": {"value": 1}
        }))
        .unwrap();
        assert_eq!(value, json!({"value": 1}));
    }

    #[test]
    fn extract_http_api_data_returns_message_on_failure() {
        let err = extract_http_api_data(json!({
            "status": 1001,
            "message": "fail",
            "data": {"reason": "bad request"}
        }))
        .unwrap_err();
        assert!(err.contains("fail"));
        assert!(err.contains("bad request"));
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
        assert!(err.contains("read /tmp/mcli_missing_input.json failed"));
    }

    #[test]
    fn with_session_id_injects_string_value() {
        let request = with_session_id(json!({"key": "user-1"}), 99).unwrap();
        assert_eq!(request["oid"], json!({"h": 0, "l": 99}));
        assert_eq!(request["key"], json!("user-1"));
    }

    fn unique_temp_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}_{nanos}.json"))
    }
}
