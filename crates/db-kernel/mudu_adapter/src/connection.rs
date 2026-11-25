//! SQLite connection management: per-session pinned connections and schema
//! initialization.

use lazy_static::lazy_static;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use rusqlite::Connection;
use scc::HashMap as SccHashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::config;

/// Pinned per-session connection.
pub(crate) type ConnectionRef = Arc<SMutex<Connection>>;

lazy_static! {
    /// Sticky per-session connections. A mudu session is a sticky connection
    /// (a multi-statement `BEGIN` … `COMMIT` transaction is issued as
    /// separate commands on one session), so `mudu_open` pins its connection
    /// here and every operation on the session goes through it; `mudu_close`
    /// removes it (dropping the connection rolls back any open transaction).
    /// The key carries the database path: session ids are SQLite rowids,
    /// which restart from 1 in every fresh database file, so an id alone
    /// only identifies a session together with the file it was opened in —
    /// keying by id alone silently routes operations to the wrong database
    /// after the configured db path changes with live sessions.
    pub(crate) static ref SESSION_CONNECTIONS: SccHashMap<(PathBuf, OID), ConnectionRef> = SccHashMap::new();
}

/// Returns the connection operations on `session_id` must use: the pinned
/// session connection when this process opened the session against the
/// currently configured database path, otherwise a fresh one (the session
/// row may live in a database file shared across processes).
pub(crate) fn connection_for(session_id: OID) -> RS<ConnectionRef> {
    let key = (config::db_path(), session_id);
    if let Some(entry) = SESSION_CONNECTIONS.get_sync(&key) {
        return Ok(Arc::clone(entry.get()));
    }
    Ok(Arc::new(SMutex::new(open_connection()?)))
}

/// Opens a SQLite connection using the configured database path.
pub fn open_connection() -> RS<Connection> {
    let path = config::db_path();
    if let Some(parent) = path.parent() {
        mudu_sys::fs::sync::sync_create_dir_all(parent)?;
    }

    let conn = Connection::open(&path)
        .map_err(|e| mudu_error!(ErrorCode::Database, "open sqlite connection error", e))?;
    // Schema DDL runs once per (process, db path): previously every
    // connection ran it, and parallel openers tripped over each other
    // ("database is locked") despite the busy timeout. The set guard is
    // HELD during the DDL: a concurrent opener must not skip ahead and
    // write into a table that does not exist yet. `journal_mode` is
    // persisted in the db file, but `busy_timeout` / `synchronous` are
    // per-connection, so those still run on every open.
    let mut paths = initialized_paths().lock()?;
    if paths.insert(path.clone()) {
        let result = initialize_schema(&conn);
        drop(paths);
        result?;
    } else {
        drop(paths);
        conn.execute_batch("PRAGMA busy_timeout = 5000; PRAGMA synchronous = NORMAL;")
            .map_err(|e| {
                mudu_error!(ErrorCode::Database, "configure sqlite connection error", e)
            })?;
    }
    Ok(conn)
}

/// db paths whose schema was already initialized by this process.
fn initialized_paths() -> &'static SMutex<std::collections::HashSet<std::path::PathBuf>> {
    static PATHS: std::sync::OnceLock<SMutex<std::collections::HashSet<std::path::PathBuf>>> =
        std::sync::OnceLock::new();
    PATHS.get_or_init(|| SMutex::new(std::collections::HashSet::new()))
}

fn initialize_schema(conn: &Connection) -> RS<()> {
    conn.execute_batch(
        r#"
        -- The database file may be shared by many connections (processes,
        -- pool sessions): wait for a busy writer instead of failing the
        -- whole connect with "database is locked" (SQLITE_BUSY).
        PRAGMA busy_timeout = 5000;
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;

        CREATE TABLE IF NOT EXISTS mudu_session (
            session_id INTEGER PRIMARY KEY,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS mudu_kv (
            k BLOB PRIMARY KEY,
            v BLOB NOT NULL
        );
        "#,
    )
    .map_err(|e| mudu_error!(ErrorCode::Database, "initialize sqlite schema error", e))?;
    Ok(())
}
