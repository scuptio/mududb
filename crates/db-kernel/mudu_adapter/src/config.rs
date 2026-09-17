//! Connection configuration resolved from the `MUDU_CONNECTION` environment variable.

use mudu_sys::sync::{SMutex, SRwLock};
use std::path::PathBuf;
use std::sync::OnceLock;

static DB_PATH_OVERRIDE: OnceLock<SRwLock<Option<PathBuf>>> = OnceLock::new();

/// Global test lock for tests that mutate connection configuration.
///
/// `MUDU_CONNECTION` and `DB_PATH_OVERRIDE` are process-global state, so any
/// test that touches them must hold this lock to avoid flaky cross-crate
/// interference when `cargo test --workspace` runs multiple test binaries in
/// parallel.
#[doc(hidden)]
pub fn test_lock() -> &'static SMutex<()> {
    static LOCK: OnceLock<SMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| SMutex::new(()))
}

/// Supported database backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    /// SQLite backend.
    Sqlite,
    /// PostgreSQL backend.
    Postgres,
    /// MySQL backend.
    MySql,
    /// Remote Mudud backend.
    Mudud,
}

/// Parsed connection configuration for a backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionConfig {
    /// SQLite connection using a file path.
    Sqlite {
        /// Path to the SQLite database file.
        path: PathBuf,
    },
    /// PostgreSQL connection using a URL.
    Postgres {
        /// PostgreSQL connection URL.
        url: String,
    },
    /// MySQL connection using a URL.
    MySql {
        /// MySQL connection URL.
        url: String,
    },
    /// Remote Mudud connection.
    Mudud {
        /// TCP address of the Mudud server.
        addr: String,
        /// HTTP address of the Mudud server.
        http_addr: String,
        /// Application name used for Mudud requests.
        app_name: String,
        /// Whether to run the Mudud session loop asynchronously.
        async_session_loop: bool,
    },
}

/// Overrides the database file path used by the SQLite backend.
pub fn set_db_path(path: impl Into<PathBuf>) {
    let lock = DB_PATH_OVERRIDE.get_or_init(|| SRwLock::new(None));
    #[expect(
        clippy::expect_used,
        reason = "lock poisoning indicates a fatal bug in a prior holder"
    )]
    {
        *lock.write().expect("db path lock poisoned") = Some(path.into());
    }
}

/// Resets the database path override. Only intended for tests.
#[doc(hidden)]
pub fn reset_db_path_override_for_test() {
    if let Some(lock) = DB_PATH_OVERRIDE.get() {
        #[expect(
            clippy::expect_used,
            reason = "lock poisoning indicates a fatal bug in a prior holder"
        )]
        {
            *lock.write().expect("db path lock poisoned") = None;
        }
    }
}

/// Returns the SQLite database file path.
pub fn db_path() -> PathBuf {
    #[expect(
        clippy::expect_used,
        reason = "lock poisoning indicates a fatal bug in a prior holder"
    )]
    if let Some(lock) = DB_PATH_OVERRIDE.get()
        && let Some(path) = lock.read().expect("db path lock poisoned").clone()
    {
        return path;
    }

    match connection() {
        ConnectionConfig::Sqlite { path } => path,
        ConnectionConfig::Postgres { .. }
        | ConnectionConfig::MySql { .. }
        | ConnectionConfig::Mudud { .. } => mudu_sys::env_var::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("mudu_debug.db"),
    }
}

/// Returns the currently configured backend driver.
pub fn driver() -> Driver {
    match connection() {
        ConnectionConfig::Sqlite { .. } => Driver::Sqlite,
        ConnectionConfig::Postgres { .. } => Driver::Postgres,
        ConnectionConfig::MySql { .. } => Driver::MySql,
        ConnectionConfig::Mudud { .. } => Driver::Mudud,
    }
}

/// Returns the PostgreSQL connection URL if configured.
pub fn postgres_url() -> Option<String> {
    match connection() {
        ConnectionConfig::Postgres { url } => Some(url),
        _ => None,
    }
}

/// Returns the MySQL connection URL if configured.
pub fn mysql_url() -> Option<String> {
    match connection() {
        ConnectionConfig::MySql { url } => Some(url),
        _ => None,
    }
}

/// Returns the Mudud TCP address if configured.
pub fn mudud_addr() -> Option<String> {
    match connection() {
        ConnectionConfig::Mudud { addr, .. } => Some(addr),
        _ => None,
    }
}

/// Returns the Mudud HTTP address if configured.
pub fn mudud_http_addr() -> Option<String> {
    match connection() {
        ConnectionConfig::Mudud { http_addr, .. } => Some(http_addr),
        _ => None,
    }
}

/// Returns the Mudud application name if configured.
pub fn mudud_app_name() -> Option<String> {
    match connection() {
        ConnectionConfig::Mudud { app_name, .. } => Some(app_name),
        _ => None,
    }
}

/// Returns whether the Mudud async session loop is enabled.
pub fn mudud_async_session_loop() -> bool {
    match connection() {
        ConnectionConfig::Mudud {
            async_session_loop, ..
        } => async_session_loop,
        _ => false,
    }
}

/// Returns the parsed connection configuration.
pub fn connection() -> ConnectionConfig {
    #[expect(
        clippy::expect_used,
        reason = "lock poisoning indicates a fatal bug in a prior holder"
    )]
    if let Some(lock) = DB_PATH_OVERRIDE.get()
        && let Some(path) = lock.read().expect("db path lock poisoned").clone()
    {
        return ConnectionConfig::Sqlite { path };
    }

    let raw = mudu_sys::env_var::var("MUDU_CONNECTION")
        .unwrap_or_else(|| "sqlite:///tmp/mududb.db".to_string());
    parse_connection(&raw)
}

fn parse_connection(raw: &str) -> ConnectionConfig {
    let normalized = raw.trim();
    let lower = normalized.to_ascii_lowercase();

    if lower.starts_with("postgres://") || lower.starts_with("postgresql://") {
        return ConnectionConfig::Postgres {
            url: normalized.to_string(),
        };
    }

    if lower.starts_with("mysql://") {
        return ConnectionConfig::MySql {
            url: normalized.to_string(),
        };
    }

    if lower.starts_with("mudud://") {
        return parse_mudud_connection(normalized);
    }

    if lower.starts_with("sqlite://") {
        let path = normalized.trim_start_matches("sqlite://");
        return ConnectionConfig::Sqlite {
            path: PathBuf::from(path),
        };
    }

    if lower.starts_with("sqlite:") {
        let path = normalized.trim_start_matches("sqlite:");
        return ConnectionConfig::Sqlite {
            path: PathBuf::from(path),
        };
    }

    ConnectionConfig::Sqlite {
        path: PathBuf::from(normalized),
    }
}

fn parse_mudud_connection(raw: &str) -> ConnectionConfig {
    let without_scheme = raw.trim_start_matches("mudud://");
    let (path_part, query_part) = without_scheme
        .split_once('?')
        .map(|(path, query)| (path.trim(), Some(query.trim())))
        .unwrap_or((without_scheme.trim(), None));
    let (addr, app_name) = path_part
        .split_once('/')
        .map(|(addr, app_name)| (addr.trim(), app_name.trim()))
        .unwrap_or((path_part.trim(), ""));
    let app_name = if app_name.is_empty() {
        "default".to_string()
    } else {
        app_name.to_string()
    };
    let http_addr = query_part
        .and_then(parse_mudud_http_addr_query)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "127.0.0.1:8300".to_string());
    let async_session_loop = query_part
        .and_then(parse_mudud_async_query)
        .unwrap_or(false);
    ConnectionConfig::Mudud {
        addr: addr.to_string(),
        http_addr,
        app_name,
        async_session_loop,
    }
}

fn parse_mudud_async_query(query: &str) -> Option<bool> {
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = key.trim();
        let value = value.trim();
        if matches!(key, "async_session_loop" | "async_sessions" | "async") {
            return Some(matches!(value, "1" | "true" | "yes" | "on"));
        }
    }
    None
}

fn parse_mudud_http_addr_query(query: &str) -> Option<String> {
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if matches!(key.trim(), "http_addr" | "http" | "admin_addr") {
            return Some(value.trim().to_string());
        }
    }
    None
}
