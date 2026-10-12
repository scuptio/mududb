use ::catalog::contract::message_bus_api::MessageBusRef;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::worker_snapshot::KvItem;
use async_trait::async_trait;
use mudu::common::id::{AttrIndex, OID};
use mudu::common::result::RS;
use mudu_binding::universal::uni_fs_dirent::UniFsDirent;
use mudu_binding::universal::uni_fs_stat::UniFsStat;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use std::cell::UnsafeCell;
use std::sync::Arc;

use ::xengine::api::{DeltaOp, XContract};
use ::xengine::DataBin;

thread_local! {
    static CURRENT_WORKER_LOCAL: UnsafeCell<Option<WorkerLocalRef>> =
        const { UnsafeCell::new(None) };
}

/// Transaction-control instruction executed on a worker session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerExecute {
    /// Begin the session's transaction.
    BeginTx,
    /// Commit the session's transaction.
    CommitTx,
    /// Roll back the session's transaction.
    RollbackTx,
}

/// The fs syscall surface a [`WorkerLocal`] can expose, abstracted behind a
/// trait object so the worker-local contract does not depend on the concrete
/// server-side `FsService` type.
#[async_trait]
pub trait WorkerFsService: Send + Sync {
    /// Open an fs object and return a file descriptor.
    async fn fs_open(&self, session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32>;

    /// Close an open fs file descriptor.
    async fn fs_close(&self, session_id: OID, fd: u32) -> RS<()>;

    /// Read up to `len` bytes at the fd cursor, advancing the cursor.
    async fn fs_read(&self, session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>>;

    /// Write `data` at the fd cursor; returns bytes written.
    async fn fs_write(&self, session_id: OID, fd: u32, data: &[u8]) -> RS<u32>;

    /// Read up to `len` bytes at `offset` without moving the cursor.
    async fn fs_pread(&self, session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>>;

    /// Write `data` at `offset` without moving the cursor.
    async fn fs_pwrite(&self, session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()>;

    /// Move the fd cursor (`whence` 0/1/2 = SET/CUR/END); returns the new cursor.
    fn fs_lseek(&self, session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64>;

    /// Stat an open fs file descriptor.
    fn fs_fstat(&self, session_id: OID, fd: u32) -> RS<UniFsStat>;

    /// Flush a write fd's content to durable storage.
    async fn fs_fsync(&self, session_id: OID, fd: u32) -> RS<()>;

    /// Stat an fs object without opening a file descriptor.
    async fn fs_stat(&self, session_id: OID, oid: OID, path: &str) -> RS<UniFsStat>;

    /// List the entries of an fs object directory.
    async fn fs_readdir(&self, session_id: OID, oid: OID, path: &str) -> RS<Vec<UniFsDirent>>;
}

/// The worker-local surface of the worker the caller runs on: session
/// lifecycle, KV access, SQL execution, and the relation/fs syscalls.
#[async_trait]
pub trait WorkerLocal: Send + Sync {
    /// The worker's engine contract.
    fn x_contract(&self) -> Arc<dyn XContract>;
    /// The worker's meta manager.
    fn meta_mgr(&self) -> Arc<dyn MetaMgr>;
    /// The server's message bus.
    fn message_bus(&self) -> MessageBusRef;

    /// Return the fs object IO service of this worker.
    ///
    /// The default reports fs syscalls as unavailable; the session-bound
    /// worker runtime overrides this with the worker's service instance.
    fn fs_service(&self) -> RS<Arc<dyn WorkerFsService>> {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "fs syscalls are not available on this worker"
        ))
    }

    /// Return the session this worker-local view is bound to, if any.
    ///
    /// The SyscallPayload v1 fs frames carry no session id outside `fs-open`:
    /// fd-based operations and `fs-stat`/`fs-readdir` resolve the session of
    /// the calling procedure through this accessor. The default reports no
    /// bound session; the session-bound worker runtime overrides it.
    fn current_session_id(&self) -> Option<OID> {
        None
    }

    /// Opens a worker session and returns its id.
    async fn open_async(&self) -> RS<OID>;

    /// Opens a session on the given worker; the default supports only the
    /// local worker (`worker_id == 0`).
    async fn open_argv_async(&self, worker_id: OID) -> RS<OID> {
        if worker_id == 0 {
            self.open_async().await
        } else {
            Err(mudu::mudu_error!(
                mudu::error::ErrorCode::NotImplemented,
                format!("worker-local open on worker {} is not supported", worker_id)
            ))
        }
    }

    /// Closes the session.
    async fn close_async(&self, session_id: OID) -> RS<()>;

    /// Runs a transaction-control instruction on the session.
    async fn execute_async(&self, session_id: OID, instruction: WorkerExecute) -> RS<()>;

    /// Puts a key-value pair inside the session.
    async fn put_async(&self, session_id: OID, key: Vec<u8>, value: Vec<u8>) -> RS<()>;

    /// Deletes a key inside the session.
    async fn delete_async(&self, session_id: OID, key: &[u8]) -> RS<()>;

    /// Reads a key inside the session.
    async fn get_async(&self, session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>>;

    /// Scans the `[start_key, end_key)` key range inside the session.
    async fn range_async(
        &self,
        session_id: OID,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>>;

    /// Run a SQL query in the schema of `app_name` (`None`/sentinels map to
    /// the default schema; see `contract::app_schema::current_schema`).
    async fn query(
        &self,
        oid: OID,
        app_name: Option<&str>,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>>;

    /// Runs a SQL command in the schema of `app_name` and returns the
    /// affected row count.
    async fn execute(
        &self,
        oid: OID,
        app_name: Option<&str>,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<u64>;

    /// Runs a multi-statement SQL batch in the schema of `app_name` and
    /// returns the affected row count.
    async fn batch(
        &self,
        oid: OID,
        app_name: Option<&str>,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<u64>;

    /// Point-read one relation row by primary key inside the session
    /// transaction, returning the projected columns in `select` order. The
    /// bare table name resolves against the schema of `app_name`.
    ///
    /// The default reports the relation syscalls as unavailable; the
    /// session-bound worker runtime overrides this with a direct
    /// [`XContract::read_key`] call that bypasses SQL parsing and result-set
    /// serialization.
    async fn relation_get(
        &self,
        session_id: OID,
        app_name: Option<&str>,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        select: Vec<AttrIndex>,
    ) -> RS<Option<Vec<Option<DataBin>>>> {
        let _ = (session_id, app_name, table, key, select);
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "relation syscalls are not available on this worker"
        ))
    }

    /// Read-modify-write one relation row by primary key inside the session
    /// transaction; `deltas` are evaluated against the latest committed row
    /// under the statement lock. Returns the affected row count. The bare
    /// table name resolves against the schema of `app_name`.
    async fn relation_update(
        &self,
        session_id: OID,
        app_name: Option<&str>,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        values: Vec<(AttrIndex, DataBin)>,
        deltas: Vec<(AttrIndex, DeltaOp, DataBin)>,
    ) -> RS<u64> {
        let _ = (session_id, app_name, table, key, values, deltas);
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "relation syscalls are not available on this worker"
        ))
    }

    /// Insert one relation row inside the session transaction; a duplicate
    /// primary key fails with `EntityAlreadyExists`. The bare table name
    /// resolves against the schema of `app_name`.
    async fn relation_insert(
        &self,
        session_id: OID,
        app_name: Option<&str>,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        values: Vec<(AttrIndex, DataBin)>,
    ) -> RS<()> {
        let _ = (session_id, app_name, table, key, values);
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "relation syscalls are not available on this worker"
        ))
    }
}

/// Shared reference to a [`WorkerLocal`].
pub type WorkerLocalRef = Arc<dyn WorkerLocal + Send + Sync>;

/// Installs the worker-local reference of the current thread (worker task
/// startup).
pub fn set_current_worker_local(worker_local: WorkerLocalRef) {
    CURRENT_WORKER_LOCAL.with(|slot| {
        // Safety: the slot is thread-local and only mutated through these helpers.
        unsafe {
            *slot.get() = Some(worker_local);
        }
    });
}

/// Removes the worker-local reference of the current thread (worker task
/// shutdown).
pub fn unset_current_worker_local() {
    CURRENT_WORKER_LOCAL.with(|slot| {
        // Safety: the slot is thread-local and only mutated through these helpers.
        unsafe {
            *slot.get() = None;
        }
    });
}

/// The worker-local reference of the current thread, when running inside a
/// worker.
pub fn try_current_worker_local() -> Option<WorkerLocalRef> {
    CURRENT_WORKER_LOCAL.with(|slot| {
        // Safety: shared reads are confined to the current thread-local slot.
        let worker_local = unsafe { &*slot.get() };
        worker_local.as_ref().cloned()
    })
}
