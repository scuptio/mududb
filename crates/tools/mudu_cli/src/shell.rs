//! Interactive SQL shell (`mcli shell`) and its line-reader abstraction.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_client::client::async_client::AsyncClient;
use mudu_client::client::json_client::JsonClient;
use serde_json::{Value, json};
use std::io::IsTerminal;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::cli::ShellArgs;
use crate::dispatch::JsonClientConnect;
use crate::io_util::print_json_to_writer;

/// Error returned by a shell line reader.
#[derive(Debug)]
pub(crate) enum ShellReadError {
    /// User interrupted input (e.g. Ctrl-C).
    Interrupted,
    /// End of input stream.
    Eof,
    /// Other I/O error.
    Other(String),
}

/// Abstraction over `rustyline::DefaultEditor` so the shell can be unit-tested
/// without a real terminal.
pub(crate) trait ShellLineReader {
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
pub(crate) struct ShellOutputOptions {
    /// Print compact JSON instead of pretty JSON.
    pub(crate) compact: bool,
    /// Render SQL query results as an interactive table.
    pub(crate) table: bool,
    /// Disable ratatui TUI rendering and always print JSON.
    pub(crate) no_table: bool,
}

pub(crate) async fn run_shell<JC, R, W>(
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
            if handle_meta_command(trimmed, &mut app, reader, output)? {
                break;
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

        execute_statement(&mut client, &app, &statement, options, output).await?;
    }

    Ok(())
}

/// Handles a `\`-prefixed meta command line. Returns `Ok(true)` when the
/// shell should quit. Swaps the per-app history file when `\app` changed the
/// active app.
fn handle_meta_command<R, W>(
    trimmed: &str,
    app: &mut String,
    reader: &mut R,
    output: &mut W,
) -> RS<bool>
where
    R: ShellLineReader,
    W: Write,
{
    let old_app = app.clone();
    if handle_shell_meta(trimmed, app, output)? {
        return Ok(true);
    }
    if old_app != *app {
        if let Some(path) = get_history_path(&old_app) {
            reader.save_history(&path);
        }
        reader.clear_history();
        if let Some(path) = get_history_path(app) {
            reader.load_history(&path);
        }
    }
    Ok(false)
}

/// Sends one complete SQL statement to the server and renders the response:
/// error JSON, an interactive/printed query table, or the affected-row count.
async fn execute_statement<C, W>(
    client: &mut JsonClient<C>,
    app: &str,
    statement: &str,
    options: ShellOutputOptions,
    output: &mut W,
) -> RS<()>
where
    C: AsyncClient,
    W: Write,
{
    let is_query = looks_like_query(statement);
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
        return Ok(());
    }

    if let Some(table_value) = mudu_cli::tui::extract_query_table(&response) {
        let interactive_tty = io::stdout().is_terminal() && io::stdin().is_terminal();
        if !options.compact && !options.no_table && (options.table || interactive_tty) {
            mudu_cli::tui::run_query_table(table_value)
                .map_err(|e| mudu_error!(ErrorCode::InvalidArgument, e))?;
        } else {
            print_json_to_writer(&response, options.compact, output)?;
        }
        return Ok(());
    }

    let affected = response
        .get("affected_rows")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    writeln!(output, "affected_rows: {affected}")
        .map_err(|e| mudu_error!(ErrorCode::Io, format!("write output failed: {e}")))?;
    Ok(())
}

pub(crate) fn get_history_path(app: &str) -> Option<PathBuf> {
    mudu_sys::env_var::var("HOME")
        .or(mudu_sys::env_var::var("USERPROFILE"))
        .map(|h| {
            let mut path = std::path::PathBuf::from(h);
            path.push(format!(".mcli_history_{}", app));
            path
        })
}

pub(crate) fn handle_shell_meta(
    input: &str,
    app: &mut String,
    output: &mut impl Write,
) -> RS<bool> {
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

pub(crate) fn statement_complete(buf: &str) -> bool {
    buf.trim_end().ends_with(';')
}

pub(crate) fn finalize_statement(buf: &str) -> String {
    buf.trim().to_string()
}

pub(crate) fn looks_like_query(sql: &str) -> bool {
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
