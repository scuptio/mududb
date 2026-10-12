//! SQLite-backed emulation core of the mock backend: database-path
//! configuration, the public frame/argv entry points and the command/query
//! error constructors. MSSP frame routing, SQLite execution and value
//! conversion live in the continuation `impl` blocks under `sqlite/`.

use crate::error::ApiError;
use crate::types::{UniCommandReturn, UniQueryReturn};
use crate::{UniCommandArgv, UniError, UniQueryArgv};
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

mod convert;
mod fs_route;
mod routing;
mod session_kv;
mod sql;

static DATABASE_PATH_OVERRIDE: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();

/// In-process mock backend answering MSSP syscall frames: SQL via SQLite
/// (`rusqlite`), session/KV/relation and fs via in-memory emulations.
pub struct MockSqliteMuduSysCall;

impl MockSqliteMuduSysCall {
    /// Overrides the SQLite database file used by the SQL emulation (takes
    /// precedence over the `MUDU_MOCK_SQLITE_PATH` environment variable).
    pub fn set_database_path(path: impl Into<PathBuf>) {
        let lock = DATABASE_PATH_OVERRIDE.get_or_init(|| RwLock::new(None));
        *lock.write().expect("database path lock poisoned") = Some(path.into());
    }

    /// Returns the SQLite database file in effect: the explicit override,
    /// `MUDU_MOCK_SQLITE_PATH`, or `./mudu_mock.db`.
    pub fn database_path() -> PathBuf {
        if let Some(lock) = DATABASE_PATH_OVERRIDE.get()
            && let Some(path) = lock.read().expect("database path lock poisoned").clone()
        {
            return path;
        }

        if let Some(path) = std::env::var_os("MUDU_MOCK_SQLITE_PATH") {
            return PathBuf::from(path);
        }

        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("mudu_mock.db")
    }

    /// Runs a `query` request frame against the SQLite emulation and
    /// returns the encoded result frame.
    pub async fn query_raw(query_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        tokio::task::spawn_blocking(move || {
            let argv = crate::mudu_sys::decode_query_request(&query_in)?;
            let result = Self::sys_query_sync(argv);
            crate::mudu_sys::encode_query_response(&result)
        })
        .await?
    }

    /// Runs a `command` request frame against the SQLite emulation and
    /// returns the encoded result frame.
    pub async fn command_raw(command_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        tokio::task::spawn_blocking(move || {
            let argv = crate::mudu_sys::decode_command_request(&command_in)?;
            let result = Self::sys_command_sync(argv);
            crate::mudu_sys::encode_command_response(&result)
        })
        .await?
    }

    /// `fetch` is not one of the 23 `uni-syscall.wit` syscalls and has no
    /// SyscallPayload v1 route: the cursor bytes pass through untouched.
    pub async fn fetch_raw(query_result: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Ok(query_result)
    }

    /// Answers a pre-encoded `batch` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn batch_raw(batch_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(batch_in).await
    }

    /// Answers a pre-encoded `open-session` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn open_raw(open_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(open_in).await
    }

    /// Answers a pre-encoded `close-session` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn close_raw(close_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(close_in).await
    }

    /// Answers a pre-encoded `get` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn get_raw(get_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(get_in).await
    }

    /// Answers a pre-encoded `put` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn put_raw(put_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(put_in).await
    }

    /// Answers a pre-encoded `delete` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn delete_raw(delete_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(delete_in).await
    }

    /// Answers a pre-encoded `range` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn range_raw(range_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(range_in).await
    }

    /// Answers a pre-encoded `relation-get` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn relation_get_raw(relation_get_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(relation_get_in).await
    }

    /// Answers a pre-encoded `relation-update` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn relation_update_raw(relation_update_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(relation_update_in).await
    }

    /// Answers a pre-encoded `relation-insert` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn relation_insert_raw(relation_insert_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(relation_insert_in).await
    }

    /// Answers a pre-encoded `fs-open` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn fs_open_raw(fs_open_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_open_in).await
    }

    /// Answers a pre-encoded `fs-close` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn fs_close_raw(fs_close_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_close_in).await
    }

    /// Answers a pre-encoded `fs-read` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn fs_read_raw(fs_read_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_read_in).await
    }

    /// Answers a pre-encoded `fs-write` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn fs_write_raw(fs_write_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_write_in).await
    }

    /// Answers a pre-encoded `fs-pread` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn fs_pread_raw(fs_pread_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_pread_in).await
    }

    /// Answers a pre-encoded `fs-pwrite` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn fs_pwrite_raw(fs_pwrite_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_pwrite_in).await
    }

    /// Answers a pre-encoded `fs-lseek` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn fs_lseek_raw(fs_lseek_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_lseek_in).await
    }

    /// Answers a pre-encoded `fs-fstat` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn fs_fstat_raw(fs_fstat_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_fstat_in).await
    }

    /// Answers a pre-encoded `fs-stat` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn fs_stat_raw(fs_stat_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_stat_in).await
    }

    /// Answers a pre-encoded `fs-fsync` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn fs_fsync_raw(fs_fsync_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_fsync_in).await
    }

    /// Answers a pre-encoded `fs-readdir` request frame by routing it to the
    /// emulation selected by the MSSP message kind.
    pub async fn fs_readdir_raw(fs_readdir_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_readdir_in).await
    }

    /// Executes a decoded `command` argv against the SQLite emulation.
    pub async fn sys_command(argv: UniCommandArgv) -> UniCommandReturn {
        tokio::task::spawn_blocking(move || Self::sys_command_sync(argv))
            .await
            .unwrap_or_else(|error| Self::command_error(error.to_string()))
    }

    /// Executes a decoded `query` argv against the SQLite emulation.
    pub async fn sys_query(argv: UniQueryArgv) -> UniQueryReturn {
        tokio::task::spawn_blocking(move || Self::sys_query_sync(argv))
            .await
            .unwrap_or_else(|error| Self::query_error(error.to_string()))
    }

    fn sys_command_sync(argv: UniCommandArgv) -> UniCommandReturn {
        match Self::try_sys_command(argv) {
            Ok(result) => UniCommandReturn::Ok(result),
            Err(message) => Self::command_error(message),
        }
    }

    fn sys_query_sync(argv: UniQueryArgv) -> UniQueryReturn {
        match Self::try_sys_query(argv) {
            Ok(result) => UniQueryReturn::Ok(result),
            Err(message) => Self::query_error(message),
        }
    }

    fn command_error(message: String) -> UniCommandReturn {
        UniCommandReturn::Err(UniError {
            err_code: 1,
            err_msg: message,
            ..Default::default()
        })
    }

    fn query_error(message: String) -> UniQueryReturn {
        UniQueryReturn::Err(UniError {
            err_code: 1,
            err_msg: message,
            ..Default::default()
        })
    }
}
