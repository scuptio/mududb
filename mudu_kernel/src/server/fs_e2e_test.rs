#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]
//! In-process end-to-end tests for the filesystem type feature.
//!
//! These tests boot a real single-worker
//! [`WorkerRuntime`](super::worker::WorkerRuntime) on temporary directories
//! and drive the full flow through SQL (DDL + DML) and the
//! [`FsService`](super::fs_service::FsService) syscalls:
//!
//! 1. Admin session creates the fs types and tables; INSERT binds a fresh fs
//!    object id into the FS column (tag `0xF5` in the top byte).
//! 2. Content is written through a write fd (including a sparse `pwrite`
//!    hole), sealed on close and committed with the transaction.
//! 3. A later transaction reads the sealed generation back through read fds,
//!    `pread`, `lseek`, `fstat` and `stat`.
//! 4. UPDATE rebinds the column to a new object (the old one becomes
//!    invisible) and DELETE unbinds it; the GC then reclaims the orphaned
//!    generation files while live objects survive.
//! 5. DIRECTORY objects support entries, `readdir`, entry reads and path
//!    escape protection; `readdir` on a FILE object is `NotADirectory`.
//! 6. A dropped (uncommitted) write generation is reclaimed by the startup
//!    recovery scan of a fresh runtime on the same directories.
//! 7. Non-admin sessions cannot run `CREATE/DROP TYPE FILESYSTEM`.
//!
//! Miri cannot execute the tree-sitter FFI behind SQL parsing, so the whole
//! module is excluded under Miri (see `mod.rs`).

use std::path::PathBuf;

use mudu::common::id::OID;
use mudu::error::ErrorCode;
use mudu_sys::env_var::temp_dir;
use mudu_sys::fs::sync::path_exists;
use mudu_utils::oid::gen_oid;

use crate::meta::fs_object::{FS_OBJECT_STATE_PENDING, FS_OBJECT_STATE_SEALED};
use crate::meta::fs_type_catalog::fs_storage_root;
use crate::server::session_bound_worker_runtime::new_session_bound_worker_runtime;
use crate::server::worker::{WorkerRuntime, WorkerRuntimeParams};
use crate::server::worker_local::{WorkerExecute, WorkerLocal};
use crate::server::worker_registry::load_or_create_worker_registry;
use crate::wal::worker_log::{WalSyncPolicy, WorkerLogBatching};

const O_RDONLY: u32 = 0;
const O_WRONLY: u32 = 1;

const WHENCE_SET: u32 = 0;
const WHENCE_CUR: u32 = 1;
const WHENCE_END: u32 = 2;

/// Temporary directories of one test runtime, removed on drop.
struct TestDirs {
    base: PathBuf,
    registry_dir: String,
    log_dir: String,
    data_dir: String,
}

impl TestDirs {
    fn new(prefix: &str) -> Self {
        let base = temp_dir().join(format!("{}_{}", prefix, gen_oid()));
        Self {
            registry_dir: base.join("registry").to_string_lossy().into_owned(),
            log_dir: base.join("log").to_string_lossy().into_owned(),
            data_dir: base.join("data").to_string_lossy().into_owned(),
            base,
        }
    }
}

impl Drop for TestDirs {
    fn drop(&mut self) {
        let _ = mudu_sys::fs::sync::remove_dir_all(&self.base);
    }
}

/// Build a single-worker runtime the way the tokio server backend does:
/// create, initialize meta/WAL, then bootstrap storage relations.
async fn build_worker(dirs: &TestDirs) -> WorkerRuntime {
    let registry = load_or_create_worker_registry(&dirs.registry_dir, 1).unwrap();
    let identity = registry.worker(0).cloned().unwrap();
    let worker = WorkerRuntime::new(WorkerRuntimeParams {
        identity,
        worker_count: 1,
        log_dir: dirs.log_dir.clone(),
        data_dir: dirs.data_dir.clone(),
        log_chunk_size: 4096,
        log_batching: WorkerLogBatching::default(),
        wal_sync_policy: WalSyncPolicy::Commit,
        procedure_runtime: None,
        registry,
        async_runtime: None,
        server_instance_id: 0,
    })
    .await
    .unwrap();
    worker.initialize().await.unwrap();
    worker.bootstrap_storage_async().await.unwrap();
    worker
}

async fn begin(local: &dyn WorkerLocal, session: OID) {
    local
        .execute_async(session, WorkerExecute::BeginTx)
        .await
        .unwrap();
}

async fn commit(local: &dyn WorkerLocal, session: OID) {
    local
        .execute_async(session, WorkerExecute::CommitTx)
        .await
        .unwrap();
}

async fn rollback(local: &dyn WorkerLocal, session: OID) {
    local
        .execute_async(session, WorkerExecute::RollbackTx)
        .await
        .unwrap();
}

async fn exec_sql(local: &dyn WorkerLocal, session: OID, sql: &str) -> u64 {
    local
        .execute(session, Box::new(sql.to_string()), Box::new(()))
        .await
        .unwrap()
}

async fn exec_sql_err(local: &dyn WorkerLocal, session: OID, sql: &str) -> mudu::error::MuduError {
    local
        .execute(session, Box::new(sql.to_string()), Box::new(()))
        .await
        .unwrap_err()
}

/// Run a single-row single-column query and return the `U128` value.
async fn query_one_oid(local: &dyn WorkerLocal, session: OID, sql: &str) -> OID {
    let result = local
        .query(session, Box::new(sql.to_string()), Box::new(()))
        .await
        .unwrap();
    let mut rows = Vec::new();
    while let Some(row) = result.next().await.unwrap() {
        rows.push(row);
    }
    assert_eq!(rows.len(), 1, "expected exactly one row for {sql:?}");
    *rows[0].values()[0]
        .as_u128()
        .expect("fs column value must be U128")
}

/// Run a query and return the number of rows.
async fn query_row_count(local: &dyn WorkerLocal, session: OID, sql: &str) -> usize {
    let result = local
        .query(session, Box::new(sql.to_string()), Box::new(()))
        .await
        .unwrap();
    let mut count = 0;
    while result.next().await.unwrap().is_some() {
        count += 1;
    }
    count
}

/// Flat-layout content path of one FILE generation: `{root}/{oidhex}.{gen}`.
fn file_generation_path(data_dir: &str, fs_id: u64, oid: OID, generation: u64) -> PathBuf {
    fs_storage_root(data_dir, fs_id).join(format!("{oid:032x}.{generation}"))
}

#[test]
fn fs_e2e_full_flow() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let dirs = TestDirs::new("fs_e2e_full");
        let worker = build_worker(&dirs).await;
        let session = worker.create_session_with_admin(1, true).unwrap();
        let local_arc = new_session_bound_worker_runtime(worker.clone(), session);
        let local: &dyn WorkerLocal = local_arc.as_ref();
        let fs = worker.fs_service();

        // --- DDL: fs types, tables, and the catalog checks around them.
        exec_sql(local, session, "CREATE TYPE FILESYSTEM FILE photo_fs").await;
        exec_sql(local, session, "CREATE TYPE FILESYSTEM DIRECTORY asset_fs").await;
        let err = exec_sql_err(local, session, "CREATE TYPE FILESYSTEM FILE photo_fs").await;
        assert_eq!(err.ec(), ErrorCode::AlreadyExists);
        exec_sql(
            local,
            session,
            "CREATE TABLE t (id BIGINT PRIMARY KEY, photo photo_fs)",
        )
        .await;
        exec_sql(
            local,
            session,
            "CREATE TABLE docs (id BIGINT PRIMARY KEY, entry asset_fs)",
        )
        .await;
        let err = exec_sql_err(
            local,
            session,
            "CREATE TABLE bad (id BIGINT PRIMARY KEY, nope nope_fs)",
        )
        .await;
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        // An explicit value for an FS column is rejected; nothing is staged.
        begin(local, session).await;
        let err = exec_sql_err(local, session, "INSERT INTO t VALUES (9, 123)").await;
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        rollback(local, session).await;

        // --- INSERT binds a fresh fs object; write + seal its content in-tx.
        begin(local, session).await;
        exec_sql(local, session, "INSERT INTO t (id) VALUES (1)").await;
        let oid = query_one_oid(local, session, "SELECT photo FROM t WHERE id = 1").await;
        assert_eq!(oid >> 120, 0xF5, "fs oid must carry the 0xF5 tag");
        let fd = fs.fs_open(session, oid, "", O_WRONLY).await.unwrap();
        assert_eq!(fs.fs_write(session, fd, b"hello").await.unwrap(), 5);
        // Positioned write past EOF leaves a sparse hole.
        fs.fs_pwrite(session, fd, 10, b"z").await.unwrap();
        fs.fs_fsync(session, fd).await.unwrap();
        let stat = fs.fs_fstat(session, fd).unwrap();
        assert_eq!(stat.state, FS_OBJECT_STATE_PENDING);
        assert_eq!(stat.generation, 1);
        assert_eq!(stat.length, 11);
        fs.fs_close(session, fd).await.unwrap();
        commit(local, session).await;

        // --- A new transaction reads the sealed generation back.
        begin(local, session).await;
        let again = query_one_oid(local, session, "SELECT photo FROM t WHERE id = 1").await;
        assert_eq!(again, oid);
        let fd = fs.fs_open(session, oid, "", O_RDONLY).await.unwrap();
        assert_eq!(fs.fs_read(session, fd, 4).await.unwrap(), b"hell".to_vec());
        // pread does not move the cursor; the sparse hole reads as zeros.
        assert_eq!(
            fs.fs_pread(session, fd, 4, 7).await.unwrap(),
            b"o\0\0\0\0\0z".to_vec()
        );
        assert_eq!(fs.fs_read(session, fd, 1).await.unwrap(), b"o".to_vec());
        // Short read at EOF, then an empty buffer signals EOF.
        assert_eq!(fs.fs_lseek(session, fd, 5, WHENCE_SET).unwrap(), 5);
        assert_eq!(
            fs.fs_read(session, fd, 16).await.unwrap(),
            b"\0\0\0\0\0z".to_vec()
        );
        assert_eq!(fs.fs_read(session, fd, 1).await.unwrap(), Vec::<u8>::new());
        assert_eq!(fs.fs_lseek(session, fd, -6, WHENCE_END).unwrap(), 5);
        assert_eq!(fs.fs_lseek(session, fd, 5, WHENCE_CUR).unwrap(), 10);
        assert_eq!(fs.fs_read(session, fd, 1).await.unwrap(), b"z".to_vec());
        let stat = fs.fs_fstat(session, fd).unwrap();
        assert_eq!(stat.state, FS_OBJECT_STATE_SEALED);
        assert_eq!(stat.generation, 1);
        assert_eq!(stat.length, 11);
        assert_eq!(stat.entry, "");
        fs.fs_close(session, fd).await.unwrap();
        let stat = fs.fs_stat(session, oid, "").await.unwrap();
        assert_eq!(stat.length, 11);
        assert_eq!(stat.state, FS_OBJECT_STATE_SEALED);
        assert_eq!(stat.generation, 1);
        commit(local, session).await;

        // --- UPDATE rebinds the FS column: new object id, old one unbound.
        begin(local, session).await;
        exec_sql(local, session, "UPDATE t SET photo = NULL WHERE id = 1").await;
        let new_oid = query_one_oid(local, session, "SELECT photo FROM t WHERE id = 1").await;
        assert_ne!(new_oid, oid);
        assert_eq!(new_oid >> 120, 0xF5);
        let fd = fs.fs_open(session, new_oid, "", O_WRONLY).await.unwrap();
        fs.fs_write(session, fd, b"v2-content").await.unwrap();
        fs.fs_close(session, fd).await.unwrap();
        commit(local, session).await;
        begin(local, session).await;
        let err = fs.fs_open(session, oid, "", O_RDONLY).await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);
        let fd = fs.fs_open(session, new_oid, "", O_RDONLY).await.unwrap();
        assert_eq!(
            fs.fs_read(session, fd, 64).await.unwrap(),
            b"v2-content".to_vec()
        );
        fs.fs_close(session, fd).await.unwrap();
        commit(local, session).await;

        // --- DELETE unbinds the object.
        begin(local, session).await;
        exec_sql(local, session, "DELETE FROM t WHERE id = 1").await;
        commit(local, session).await;
        begin(local, session).await;
        let err = fs
            .fs_open(session, new_oid, "", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);
        commit(local, session).await;

        // --- DIRECTORY object: entries, readdir, entry reads, protections.
        begin(local, session).await;
        exec_sql(local, session, "INSERT INTO docs (id) VALUES (1)").await;
        let dir_oid = query_one_oid(local, session, "SELECT entry FROM docs WHERE id = 1").await;
        for (entry, payload) in [("a", b"A".as_slice()), ("dir/b", b"BB".as_slice())] {
            let fd = fs.fs_open(session, dir_oid, entry, O_WRONLY).await.unwrap();
            fs.fs_write(session, fd, payload).await.unwrap();
            fs.fs_close(session, fd).await.unwrap();
        }
        commit(local, session).await;
        begin(local, session).await;
        let entries = fs.fs_readdir(session, dir_oid, "").await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "a");
        assert!(!entries[0].is_dir);
        assert_eq!(entries[0].length, 1);
        assert_eq!(entries[1].name, "dir");
        assert!(entries[1].is_dir);
        let entries = fs.fs_readdir(session, dir_oid, "dir").await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "b");
        assert!(!entries[0].is_dir);
        assert_eq!(entries[0].length, 2);
        let fd = fs
            .fs_open(session, dir_oid, "dir/b", O_RDONLY)
            .await
            .unwrap();
        assert_eq!(fs.fs_read(session, fd, 8).await.unwrap(), b"BB".to_vec());
        fs.fs_close(session, fd).await.unwrap();
        // A path escaping the object root is denied.
        let err = fs
            .fs_open(session, dir_oid, "../escape", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::PermissionDenied);
        commit(local, session).await;
        // readdir on a FILE object is rejected.
        begin(local, session).await;
        exec_sql(local, session, "INSERT INTO t (id) VALUES (2)").await;
        let file_oid = query_one_oid(local, session, "SELECT photo FROM t WHERE id = 2").await;
        let err = fs.fs_readdir(session, file_oid, "").await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotADirectory);
        commit(local, session).await;

        // --- Forged oids (with and without the 0xF5 tag) are NotFound.
        begin(local, session).await;
        let err = fs
            .fs_open(session, 0x1234u128, "", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);
        let forged = (0xF5u128 << 120) | 0xdead_beefu128;
        let err = fs.fs_open(session, forged, "", O_RDONLY).await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);
        commit(local, session).await;

        // --- GC reclaims the generations of deleted objects and keeps live ones.
        worker.fs_gc().gc_round().await.unwrap();
        let photo_fs_id = fs_id_of(&worker, "photo_fs").await;
        let asset_fs_id = fs_id_of(&worker, "asset_fs").await;
        assert!(!path_exists(file_generation_path(
            &dirs.data_dir,
            photo_fs_id,
            oid,
            1
        )));
        assert!(!path_exists(file_generation_path(
            &dirs.data_dir,
            photo_fs_id,
            new_oid,
            1
        )));
        let asset_root = fs_storage_root(&dirs.data_dir, asset_fs_id);
        assert!(path_exists(asset_root.join(format!("{dir_oid:032x}.1.a"))));
        assert!(path_exists(
            asset_root.join(format!("{dir_oid:032x}.1.dir")).join("b")
        ));

        // --- DROP TYPE is refused while a column references the fs type.
        let err = exec_sql_err(local, session, "DROP TYPE photo_fs").await;
        assert_eq!(err.ec(), ErrorCode::InvalidState);
        exec_sql(local, session, "DROP TABLE t").await;
        exec_sql(local, session, "DROP TYPE photo_fs").await;
        let err = exec_sql_err(local, session, "DROP TYPE photo_fs").await;
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);

        // --- Non-admin sessions cannot run fs type DDL.
        let plain = worker.create_session(77).unwrap();
        let plain_arc = new_session_bound_worker_runtime(worker.clone(), plain);
        let plain_local: &dyn WorkerLocal = plain_arc.as_ref();
        let err = exec_sql_err(plain_local, plain, "CREATE TYPE FILESYSTEM FILE denied_fs").await;
        assert_eq!(err.ec(), ErrorCode::PermissionDenied);
        let err = exec_sql_err(plain_local, plain, "DROP TYPE asset_fs").await;
        assert_eq!(err.ec(), ErrorCode::PermissionDenied);
    })
    .unwrap()
}

#[test]
fn fs_e2e_crash_recovery_reclaims_uncommitted_generation() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let dirs = TestDirs::new("fs_e2e_crash");
        let oid;
        let fs_id;
        let orphan_path;
        {
            let worker = build_worker(&dirs).await;
            let session = worker.create_session_with_admin(1, true).unwrap();
            let local_arc = new_session_bound_worker_runtime(worker.clone(), session);
            let local: &dyn WorkerLocal = local_arc.as_ref();
            exec_sql(local, session, "CREATE TYPE FILESYSTEM FILE photo_fs").await;
            exec_sql(
                local,
                session,
                "CREATE TABLE t (id BIGINT PRIMARY KEY, photo photo_fs)",
            )
            .await;
            fs_id = fs_id_of(&worker, "photo_fs").await;
            begin(local, session).await;
            exec_sql(local, session, "INSERT INTO t (id) VALUES (1)").await;
            oid = query_one_oid(local, session, "SELECT photo FROM t WHERE id = 1").await;
            let fs = worker.fs_service();
            let fd = fs.fs_open(session, oid, "", O_WRONLY).await.unwrap();
            fs.fs_write(session, fd, b"orphan").await.unwrap();
            // The close seals the row only into the still-open transaction.
            fs.fs_close(session, fd).await.unwrap();
            orphan_path = file_generation_path(&dirs.data_dir, fs_id, oid, 1);
            assert!(path_exists(&orphan_path));
            // Drain queued WAL frames before the simulated crash; production
            // workers drive the group-commit flush continuously.
            if let Some(log) = worker.worker_log().unwrap() {
                log.force_flush_log_async().await.unwrap();
            }
            // Simulated crash: the runtime is dropped without a commit.
        }

        // A fresh runtime on the same directories runs the recovery scan.
        let worker = build_worker(&dirs).await;
        worker.fs_gc_recover_scan().await.unwrap();
        assert!(
            !path_exists(&orphan_path),
            "uncommitted generation must be reclaimed: {}",
            orphan_path.display()
        );
        // The uncommitted row is invisible to the new runtime.
        let session = worker.create_session_with_admin(2, true).unwrap();
        let local_arc = new_session_bound_worker_runtime(worker.clone(), session);
        let local: &dyn WorkerLocal = local_arc.as_ref();
        assert_eq!(
            query_row_count(local, session, "SELECT photo FROM t WHERE id = 1").await,
            0
        );
    })
    .unwrap()
}

async fn fs_id_of(worker: &WorkerRuntime, name: &str) -> u64 {
    worker
        .meta_mgr()
        .get_fs_type_by_name(name)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("fs type {name} must be registered"))
        .fs_id()
}
