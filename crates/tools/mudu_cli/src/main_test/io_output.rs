//! Tests for JSON loading, request-body helpers, and output formatting.

use super::*;
use mudu_contract::procedure::proc_desc::ProcDesc;
use serde_json::json;
use std::io::{self, IsTerminal};

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
// Tests merged from the former inline `mod tests` in `main.rs`
// ---------------------------------------------------------------------------

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
                    "data_type": {
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
                    "data_type": {
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
    let request = with_invoke_session_id(json!({"procedure_name": "app/mod/proc"}), 99).unwrap();
    assert_eq!(request["session_id"], json!("99"));
    assert_eq!(request["procedure_name"], json!("app/mod/proc"));
}

fn unique_temp_path(prefix: &str) -> PathBuf {
    use std::time::UNIX_EPOCH;
    let nanos = mudu_sys::time::system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    mudu_sys::env_var::temp_dir().join(format!("{prefix}_{nanos}.json"))
}
