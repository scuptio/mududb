//! Tests for the interactive shell: line reader, meta commands, `run_shell`.

use super::*;
use std::path::Path;

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
// Tests merged from the former inline `mod tests` in `main.rs`
// ---------------------------------------------------------------------------

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
