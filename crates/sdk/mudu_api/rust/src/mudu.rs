use crate::error::ApiError;
use crate::mudu_sys;
use crate::mudu_sys::kv::KvPair;
use crate::mudu_sys::relation::{RelationColumn, RelationRow};
use crate::types::{UniCommandResult, UniCommandReturn, UniQueryReturn, UniReturn};
use crate::{
    UniCommandArgv, UniError, UniFsDirent, UniFsOpenArgv, UniFsStat, UniOid, UniQueryArgv,
    UniQueryResult, UniRecordType, UniRelationDelta, UniResultSet,
};

pub struct Mudu;

impl Mudu {
    pub async fn command(argv: &UniCommandArgv) -> Result<CommandResponse, ApiError> {
        Ok(CommandResponse::new(mudu_sys::sys_command(argv).await?))
    }

    pub async fn query(argv: &UniQueryArgv) -> Result<QueryResponse, ApiError> {
        Ok(QueryResponse::new(mudu_sys::sys_query(argv).await?))
    }

    pub fn serialize_command(argv: &UniCommandArgv) -> Result<Vec<u8>, ApiError> {
        mudu_sys::serialize_command(argv)
    }

    pub fn serialize_query(argv: &UniQueryArgv) -> Result<Vec<u8>, ApiError> {
        mudu_sys::serialize_query(argv)
    }

    pub fn deserialize_command(bytes: &[u8]) -> Result<UniCommandReturn, ApiError> {
        mudu_sys::deserialize_command_result(bytes)
    }

    pub fn deserialize_query(bytes: &[u8]) -> Result<UniQueryReturn, ApiError> {
        mudu_sys::deserialize_query_result(bytes)
    }

    /// Runs a `batch` syscall: a batched SQL command with the same argument
    /// and result shapes as [`Mudu::command`].
    pub async fn batch(argv: &UniCommandArgv) -> Result<CommandResponse, ApiError> {
        Ok(CommandResponse::new(
            mudu_sys::batch::sys_batch(argv).await?,
        ))
    }

    /// Runs an `open-session` syscall and returns the new session OID.
    pub async fn open_session(worker_id: &UniOid) -> Result<UniResponse<UniOid>, ApiError> {
        Ok(UniResponse::new(
            mudu_sys::session::sys_open(worker_id).await?,
        ))
    }

    /// Runs a `close-session` syscall.
    pub async fn close_session(oid: &UniOid) -> Result<UniResponse<()>, ApiError> {
        Ok(UniResponse::new(mudu_sys::session::sys_close(oid).await?))
    }

    /// Runs a `get` syscall and returns the optional value.
    pub async fn get(oid: &UniOid, key: &[u8]) -> Result<UniResponse<Option<Vec<u8>>>, ApiError> {
        Ok(UniResponse::new(mudu_sys::kv::sys_get(oid, key).await?))
    }

    /// Runs a `put` syscall.
    pub async fn put(oid: &UniOid, key: &[u8], value: &[u8]) -> Result<UniResponse<()>, ApiError> {
        Ok(UniResponse::new(
            mudu_sys::kv::sys_put(oid, key, value).await?,
        ))
    }

    /// Runs a `delete` syscall.
    pub async fn delete(oid: &UniOid, key: &[u8]) -> Result<UniResponse<()>, ApiError> {
        Ok(UniResponse::new(mudu_sys::kv::sys_delete(oid, key).await?))
    }

    /// Runs a `range` syscall and returns the key/value pairs in
    /// `[start, end)`.
    pub async fn range(
        oid: &UniOid,
        start: &[u8],
        end: &[u8],
    ) -> Result<UniResponse<Vec<KvPair>>, ApiError> {
        Ok(UniResponse::new(
            mudu_sys::kv::sys_range(oid, start, end).await?,
        ))
    }

    /// Runs a `relation-get` syscall and returns the optional projected row.
    pub async fn relation_get(
        oid: &UniOid,
        table: &str,
        key: &[RelationColumn],
        select: &[u64],
    ) -> Result<UniResponse<RelationRow>, ApiError> {
        Ok(UniResponse::new(
            mudu_sys::relation::sys_relation_get(oid, table, key, select).await?,
        ))
    }

    /// Runs a `relation-update` syscall and returns the affected row count.
    pub async fn relation_update(
        oid: &UniOid,
        table: &str,
        key: &[RelationColumn],
        values: &[RelationColumn],
        deltas: &[UniRelationDelta],
    ) -> Result<UniResponse<u64>, ApiError> {
        Ok(UniResponse::new(
            mudu_sys::relation::sys_relation_update(oid, table, key, values, deltas).await?,
        ))
    }

    /// Runs a `relation-insert` syscall; duplicate keys fail.
    pub async fn relation_insert(
        oid: &UniOid,
        table: &str,
        key: &[RelationColumn],
        values: &[RelationColumn],
    ) -> Result<UniResponse<()>, ApiError> {
        Ok(UniResponse::new(
            mudu_sys::relation::sys_relation_insert(oid, table, key, values).await?,
        ))
    }

    /// Runs an `fs-open` syscall and returns the new file descriptor.
    pub async fn fs_open(argv: &UniFsOpenArgv) -> Result<UniResponse<u32>, ApiError> {
        Ok(UniResponse::new(mudu_sys::fs::sys_fs_open(argv).await?))
    }

    /// Runs an `fs-close` syscall.
    pub async fn fs_close(fd: u32) -> Result<UniResponse<()>, ApiError> {
        Ok(UniResponse::new(mudu_sys::fs::sys_fs_close(fd).await?))
    }

    /// Runs an `fs-read` syscall and returns the read bytes.
    pub async fn fs_read(fd: u32, len: u32) -> Result<UniResponse<Vec<u8>>, ApiError> {
        Ok(UniResponse::new(mudu_sys::fs::sys_fs_read(fd, len).await?))
    }

    /// Runs an `fs-write` syscall and returns the written byte count.
    pub async fn fs_write(fd: u32, data: &[u8]) -> Result<UniResponse<u32>, ApiError> {
        Ok(UniResponse::new(
            mudu_sys::fs::sys_fs_write(fd, data).await?,
        ))
    }

    /// Runs an `fs-pread` syscall and returns the read bytes.
    pub async fn fs_pread(
        fd: u32,
        offset: u64,
        len: u32,
    ) -> Result<UniResponse<Vec<u8>>, ApiError> {
        Ok(UniResponse::new(
            mudu_sys::fs::sys_fs_pread(fd, offset, len).await?,
        ))
    }

    /// Runs an `fs-pwrite` syscall.
    pub async fn fs_pwrite(fd: u32, offset: u64, data: &[u8]) -> Result<UniResponse<()>, ApiError> {
        Ok(UniResponse::new(
            mudu_sys::fs::sys_fs_pwrite(fd, offset, data).await?,
        ))
    }

    /// Runs an `fs-lseek` syscall and returns the new cursor position.
    pub async fn fs_lseek(fd: u32, offset: i64, whence: u32) -> Result<UniResponse<u64>, ApiError> {
        Ok(UniResponse::new(
            mudu_sys::fs::sys_fs_lseek(fd, offset, whence).await?,
        ))
    }

    /// Runs an `fs-fstat` syscall and returns the stat record.
    pub async fn fs_fstat(fd: u32) -> Result<UniResponse<UniFsStat>, ApiError> {
        Ok(UniResponse::new(mudu_sys::fs::sys_fs_fstat(fd).await?))
    }

    /// Runs an `fs-stat` syscall and returns the stat record.
    pub async fn fs_stat(oid: &UniOid, path: &str) -> Result<UniResponse<UniFsStat>, ApiError> {
        Ok(UniResponse::new(
            mudu_sys::fs::sys_fs_stat(oid, path).await?,
        ))
    }

    /// Runs an `fs-fsync` syscall.
    pub async fn fs_fsync(fd: u32) -> Result<UniResponse<()>, ApiError> {
        Ok(UniResponse::new(mudu_sys::fs::sys_fs_fsync(fd).await?))
    }

    /// Runs an `fs-readdir` syscall and returns the directory entries.
    pub async fn fs_readdir(
        oid: &UniOid,
        path: &str,
    ) -> Result<UniResponse<Vec<UniFsDirent>>, ApiError> {
        Ok(UniResponse::new(
            mudu_sys::fs::sys_fs_readdir(oid, path).await?,
        ))
    }
}

/// Generic syscall response in the style of [`CommandResponse`], wrapping the
/// wire-level `result<T, uni-error>` shared by the syscalls that have no
/// dedicated `Uni*Return` enum.
#[derive(Debug, Clone)]
pub struct UniResponse<T> {
    inner: UniReturn<T>,
}

impl<T> UniResponse<T> {
    pub fn new(inner: UniReturn<T>) -> Self {
        Self { inner }
    }

    pub fn raw(&self) -> &UniReturn<T> {
        &self.inner
    }

    pub fn is_ok(&self) -> bool {
        self.inner.is_ok()
    }

    pub fn is_err(&self) -> bool {
        self.inner.is_err()
    }

    pub fn result(&self) -> Option<&T> {
        self.inner.as_ref().ok()
    }

    pub fn error(&self) -> Option<&UniError> {
        self.inner.as_ref().err()
    }

    pub fn require_ok(self) -> Result<T, UniError> {
        self.inner
    }
}

#[derive(Debug, Clone)]
pub struct CommandResponse {
    inner: UniCommandReturn,
}

impl CommandResponse {
    pub fn new(inner: UniCommandReturn) -> Self {
        Self { inner }
    }

    pub fn raw(&self) -> &UniCommandReturn {
        &self.inner
    }

    pub fn is_ok(&self) -> bool {
        matches!(self.inner, UniCommandReturn::Ok(_))
    }

    pub fn is_err(&self) -> bool {
        matches!(self.inner, UniCommandReturn::Err(_))
    }

    pub fn result(&self) -> Option<&UniCommandResult> {
        match &self.inner {
            UniCommandReturn::Ok(result) => Some(result),
            UniCommandReturn::Err(_) => None,
        }
    }

    pub fn error(&self) -> Option<&UniError> {
        match &self.inner {
            UniCommandReturn::Ok(_) => None,
            UniCommandReturn::Err(error) => Some(error),
        }
    }

    pub fn affected_rows(&self) -> Option<u64> {
        self.result().map(|result| result.affected_rows)
    }

    pub fn require_ok(self) -> Result<UniCommandResult, UniError> {
        match self.inner {
            UniCommandReturn::Ok(result) => Ok(result),
            UniCommandReturn::Err(error) => Err(error),
        }
    }
}

#[derive(Debug, Clone)]
pub struct QueryResponse {
    inner: UniQueryReturn,
}

impl QueryResponse {
    pub fn new(inner: UniQueryReturn) -> Self {
        Self { inner }
    }

    pub fn raw(&self) -> &UniQueryReturn {
        &self.inner
    }

    pub fn is_ok(&self) -> bool {
        matches!(self.inner, UniQueryReturn::Ok(_))
    }

    pub fn is_err(&self) -> bool {
        matches!(self.inner, UniQueryReturn::Err(_))
    }

    pub fn result(&self) -> Option<&UniQueryResult> {
        match &self.inner {
            UniQueryReturn::Ok(result) => Some(result),
            UniQueryReturn::Err(_) => None,
        }
    }

    pub fn error(&self) -> Option<&UniError> {
        match &self.inner {
            UniQueryReturn::Ok(_) => None,
            UniQueryReturn::Err(error) => Some(error),
        }
    }

    pub fn tuple_desc(&self) -> Option<&UniRecordType> {
        self.result().map(|result| &result.tuple_desc)
    }

    pub fn result_set(&self) -> Option<&UniResultSet> {
        self.result().map(|result| &result.result_set)
    }

    pub fn require_ok(self) -> Result<UniQueryResult, UniError> {
        match self.inner {
            UniQueryReturn::Ok(result) => Ok(result),
            UniQueryReturn::Err(error) => Err(error),
        }
    }
}
