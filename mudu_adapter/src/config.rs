use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

static DB_PATH_OVERRIDE: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    Sqlite,
    Postgres,
    MySql,
    Mudud,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionConfig {
    Sqlite {
        path: PathBuf,
    },
    Postgres {
        url: String,
    },
    MySql {
        url: String,
    },
    Mudud {
        addr: String,
        http_addr: String,
        app_name: String,
        async_session_loop: bool,
    },
}

pub fn set_db_path(path: impl Into<PathBuf>) {
    let lock = DB_PATH_OVERRIDE.get_or_init(|| RwLock::new(None));
    *lock.write().expect("db path lock poisoned") = Some(path.into());
}

pub fn db_path() -> PathBuf {
    if let Some(lock) = DB_PATH_OVERRIDE.get() {
        if let Some(path) = lock.read().expect("db path lock poisoned").clone() {
            return path;
        }
    }

    match connection() {
        ConnectionConfig::Sqlite { path } => path,
        ConnectionConfig::Postgres { .. }
        | ConnectionConfig::MySql { .. }
        | ConnectionConfig::Mudud { .. } => std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("mudu_debug.db"),
    }
}

pub fn driver() -> Driver {
    match connection() {
        ConnectionConfig::Sqlite { .. } => Driver::Sqlite,
        ConnectionConfig::Postgres { .. } => Driver::Postgres,
        ConnectionConfig::MySql { .. } => Driver::MySql,
        ConnectionConfig::Mudud { .. } => Driver::Mudud,
    }
}

pub fn postgres_url() -> Option<String> {
    match connection() {
        ConnectionConfig::Postgres { url } => Some(url),
        _ => None,
    }
}

pub fn mysql_url() -> Option<String> {
    match connection() {
        ConnectionConfig::MySql { url } => Some(url),
        _ => None,
    }
}

pub fn mudud_addr() -> Option<String> {
    match connection() {
        ConnectionConfig::Mudud { addr, .. } => Some(addr),
        _ => None,
    }
}

pub fn mudud_http_addr() -> Option<String> {
    match connection() {
        ConnectionConfig::Mudud { http_addr, .. } => Some(http_addr),
        _ => None,
    }
}

pub fn mudud_app_name() -> Option<String> {
    match connection() {
        ConnectionConfig::Mudud { app_name, .. } => Some(app_name),
        _ => None,
    }
}

pub fn mudud_async_session_loop() -> bool {
    match connection() {
        ConnectionConfig::Mudud {
            async_session_loop, ..
        } => async_session_loop,
        _ => false,
    }
}

pub fn connection() -> ConnectionConfig {
    if let Some(lock) = DB_PATH_OVERRIDE.get() {
        if let Some(path) = lock.read().expect("db path lock poisoned").clone() {
            return ConnectionConfig::Sqlite { path };
        }
    }

    let raw =
        std::env::var("MUDU_CONNECTION").unwrap_or_else(|_| "sqlite://./mudu_debug.db".to_string());
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
