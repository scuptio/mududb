//! Edge-case tests for the `mududb` facade (`db` / `fs` / `result`), run
//! against the in-process standalone adapter (SQLite + local fs emulation).
//! Complements `facade_scenario.rs` (the cross-language happy path) with the
//! error paths and the facade entry points the scenario does not touch.
//!
//! The adapter keeps process-global state (the SQLite file and the fd table),
//! so every case here is serialized on one lock and uses its own table name /
//! fs object id to stay rerunnable.
#![allow(clippy::unwrap_used)]

use mududb::contract::database::sql_param_value::SQLParamValue;
use mududb::db::Database;
use mududb::fs::{
    FS_O_RDONLY, FS_O_RDWR, FS_O_WRONLY, FS_SEEK_CUR, FS_SEEK_END, FS_SEEK_SET, fs_close, fs_fstat,
    fs_fsync, fs_lseek, fs_open, fs_pread, fs_pwrite, fs_read, fs_readdir, fs_stat, fs_write,
};
use mududb::sys::sync::SMutex;
use std::sync::LazyLock;

static LOCK: LazyLock<SMutex<()>> = LazyLock::new(|| SMutex::new(()));

fn no_params() -> SQLParamValue {
    SQLParamValue::from_vec(vec![])
}

fn lock() -> mududb::sys::sync::SMutexGuard<'static, ()> {
    LOCK.lock().unwrap()
}

/// `open_uri` with an empty URI is the default worker; a decimal URI selects
/// the worker object id (the adapter ignores it and opens a fresh session).
#[test]
fn db_open_uri_variants() {
    let _guard = lock();
    let db_default = Database::open_uri("").unwrap();
    let db_worker0 = Database::open_uri("0").unwrap();
    assert_ne!(db_default.id(), db_worker0.id());

    let mut rs = db_worker0.query(&"SELECT 1", &no_params()).unwrap();
    assert!(rs.next());
    assert_eq!(rs.current_row().unwrap().value(0).unwrap().expect_i64(), &1);

    db_default.close().unwrap();
    db_worker0.close().unwrap();
}

/// `batch` runs several statements in one syscall and reports the affected
/// rows of the last one; malformed SQL and unknown tables surface as errors.
#[test]
fn db_batch_and_error_paths() {
    let _guard = lock();
    let db = Database::open().unwrap();

    let affected = db
        .batch(
            &"DROP TABLE IF EXISTS t_edge_batch;
              CREATE TABLE t_edge_batch (id INTEGER PRIMARY KEY);
              INSERT INTO t_edge_batch VALUES (1)",
            &no_params(),
        )
        .unwrap();
    assert_eq!(affected, 1);

    assert!(db.command(&"THIS IS NOT SQL", &no_params()).is_err());
    assert!(
        db.query(&"SELECT * FROM t_edge_missing", &no_params())
            .is_err()
    );

    db.close().unwrap();
}

/// `ResultSet` edge behavior over the real wire: an empty result starts at
/// eof with no current row, and a SQL NULL survives the roundtrip.
#[test]
fn db_result_edges_over_the_wire() {
    let _guard = lock();
    let db = Database::open().unwrap();
    db.command(&"DROP TABLE IF EXISTS t_edge_result", &no_params())
        .unwrap();
    db.command(
        &"CREATE TABLE t_edge_result (id INTEGER PRIMARY KEY, name TEXT)",
        &no_params(),
    )
    .unwrap();

    // Zero rows: next() is immediately false, eof, current_row errors, but
    // the column metadata is still present.
    let mut rs = db
        .query(
            &"SELECT id, name FROM t_edge_result WHERE 1 = 0",
            &no_params(),
        )
        .unwrap();
    assert_eq!(rs.column_count(), 2);
    assert_eq!(rs.column_name(1).unwrap(), "name");
    assert!(!rs.next());
    assert!(rs.eof());
    assert!(rs.current_row().is_err());

    // A NULL value roundtrips as DataValue::null.
    db.command(&"INSERT INTO t_edge_result VALUES (1, NULL)", &no_params())
        .unwrap();
    let mut rs = db
        .query(&"SELECT name FROM t_edge_result", &no_params())
        .unwrap();
    assert!(rs.next());
    let row = rs.current_row().unwrap();
    assert!(row.is_null(0).unwrap());
    assert!(row.value(0).unwrap().is_null());

    db.close().unwrap();
}

/// fs edge cases: read-write mode, positional I/O without cursor movement,
/// all three whences, stat by fd and by path, fsync, and the error paths.
#[test]
fn fs_edge_cases() {
    let _guard = lock();
    let db = Database::open().unwrap();
    let session = db.id();
    let oid = 0xE1;

    // Clean slate: the adapter fs persists under `{db}.fs/` across runs.
    let fd = fs_open(session, oid, "edge.bin", FS_O_WRONLY).unwrap();
    fs_close(session, fd).unwrap();

    // FS_O_RDWR: write, seek back, read from the same fd.
    let fd = fs_open(session, oid, "edge.bin", FS_O_RDWR).unwrap();
    assert_eq!(fs_write(session, fd, b"hello world").unwrap(), 11);
    assert_eq!(fs_lseek(session, fd, 0, FS_SEEK_SET).unwrap(), 0);
    assert_eq!(fs_read(session, fd, 5).unwrap(), b"hello");

    // pwrite/pread use absolute offsets and do not move the cursor.
    fs_pwrite(session, fd, 6, b"WORLD").unwrap();
    assert_eq!(fs_lseek(session, fd, 0, FS_SEEK_CUR).unwrap(), 5);
    assert_eq!(fs_pread(session, fd, 6, 5).unwrap(), b"WORLD");
    assert_eq!(fs_lseek(session, fd, 0, FS_SEEK_CUR).unwrap(), 5);

    // SEEK_END / SEEK_CUR positioning.
    assert_eq!(fs_lseek(session, fd, 0, FS_SEEK_END).unwrap(), 11);
    assert_eq!(fs_lseek(session, fd, -5, FS_SEEK_CUR).unwrap(), 6);
    // Reading at EOF yields an empty buffer.
    assert_eq!(fs_lseek(session, fd, 0, FS_SEEK_END).unwrap(), 11);
    assert!(fs_read(session, fd, 1).unwrap().is_empty());

    // Stat via fd and via path agree; fsync succeeds on a write fd.
    assert_eq!(fs_fstat(session, fd).unwrap().length, 11);
    fs_fsync(session, fd).unwrap();
    fs_close(session, fd).unwrap();
    assert_eq!(fs_stat(session, oid, "edge.bin").unwrap().length, 11);

    // readdir lists the entry; a missing directory is an error.
    let entries = fs_readdir(session, oid, "").unwrap();
    assert!(entries.iter().any(|e| e.name == "edge.bin" && !e.is_dir));
    assert!(fs_readdir(session, oid, "no-such-dir").is_err());

    // Error paths: missing file under O_RDONLY, unknown fds.
    assert!(fs_open(session, oid, "no-such-file.bin", FS_O_RDONLY).is_err());
    assert!(fs_close(session, u32::MAX).is_err());
    assert!(fs_read(session, u32::MAX, 1).is_err());
    assert!(fs_fstat(session, u32::MAX).is_err());

    db.close().unwrap();
}
