//! JSON/stdin/file loading, request-body helpers, and output printing.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::tuple::datum_desc::DatumDesc;
use serde_json::{Value, json};
use std::io::IsTerminal;
use std::io::{self, Read, Write};
use std::path::PathBuf;

use crate::cli::{JsonRequestArgs, ReplPromoteArgs};

pub(crate) fn load_json_request(args: JsonRequestArgs) -> RS<Value> {
    let raw = load_required_text(args.json, args.json_file)?;
    serde_json::from_str(&raw)
        .map_err(|e| mudu_error!(ErrorCode::Decode, format!("invalid JSON request: {}", e)))
}

/// Validates the fencing precondition of `repl promote`: run the fencing
/// command when given (abort on a non-zero exit, without contacting the
/// server), else require the explicit `--yes` confirmation.
pub(crate) fn check_fencing(args: &ReplPromoteArgs) -> RS<bool> {
    if let Some(cmd) = &args.fencing_cmd {
        let status = mudu_sys::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .status()
            .map_err(|e| {
                mudu_error!(
                    ErrorCode::Io,
                    format!("run fencing command \"{cmd}\" failed: {e}")
                )
            })?;
        if !status.success() {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                format!(
                    "fencing command \"{cmd}\" exited with {status}; the old primary may not be isolated, promote aborted"
                )
            ));
        }
        return Ok(true);
    }
    if !args.yes {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "promote requires --fencing-cmd <script> or --yes (confirm the old primary is fenced)"
        ));
    }
    Ok(true)
}

/// Parses a `host:port` endpoint.
pub(crate) fn parse_host_port(addr: &str) -> RS<(String, u16)> {
    let Some((host, port)) = addr.rsplit_once(':') else {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("expected host:port, got \"{addr}\"")
        ));
    };
    let port = port
        .parse::<u16>()
        .map_err(|e| mudu_error!(ErrorCode::Parse, format!("invalid port in \"{addr}\""), e))?;
    Ok((host.to_string(), port))
}

pub(crate) fn load_required_text(inline: Option<String>, file: Option<PathBuf>) -> RS<String> {
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

pub(crate) fn with_oid(request: Value, session_id: u128) -> RS<Value> {
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

pub(crate) fn with_invoke_session_id(request: Value, session_id: u128) -> RS<Value> {
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

pub(crate) fn read_text_path(path: &PathBuf) -> RS<String> {
    if path.as_os_str() == "-" {
        read_stdin_to_string()
    } else {
        mudu_sys::fs::sync::sync_read_to_string(path)
    }
}

fn read_stdin_to_string() -> RS<String> {
    read_to_string(&mut io::stdin())
}

pub(crate) fn read_to_string(reader: &mut impl Read) -> RS<String> {
    let mut buf = String::new();
    reader
        .read_to_string(&mut buf)
        .map_err(|e| mudu_error!(ErrorCode::Io, format!("read stdin failed: {}", e)))?;
    if buf.is_empty() {
        return Err(mudu_error!(ErrorCode::InvalidArgument, "stdin is empty"));
    }
    Ok(buf)
}

pub(crate) fn print_json_to_writer(
    value: &Value,
    compact: bool,
    writer: &mut impl Write,
) -> RS<()> {
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

pub(crate) fn print_output(value: &Value, compact: bool, table: bool, no_table: bool) -> RS<()> {
    print_output_to_writer(value, compact, table, no_table, &mut io::stdout())
}

pub(crate) fn print_output_to_writer(
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

pub(crate) fn to_param(
    argv: &serde_json::Map<String, Value>,
    desc: &[DatumDesc],
) -> RS<ProcedureParam> {
    let mut vec = vec![];
    for datum_desc in desc {
        let value = argv.get(datum_desc.name()).cloned().ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidArgument,
                format!("missing parameter {}", datum_desc.name())
            )
        })?;
        let data_value = datum_desc.type_family().fn_input_json()(&value, datum_desc.data_type())
            .map_err(|e| {
            mudu_error!(
                ErrorCode::Decode,
                format!("convert parameter {} failed: {}", datum_desc.name(), e)
            )
        })?;
        vec.push(data_value);
    }
    Ok(ProcedureParam::new(0, 0, vec))
}
