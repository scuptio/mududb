//! Cross-language facade consistency scenario
//! (`doc/dev/binding_api_surface.md`, section "Behavioral consistency
//! scenario"). Every step must stay assertion-identical with the C#
//! `FacadeScenarioTests`. Runs against the in-process standalone adapter
//! (SQLite + local fs).
#![allow(clippy::unwrap_used)]

use mududb::contract::database::sql_param_value::SQLParamValue;
use mududb::db::Database;
use mududb::fs::{
    FS_O_RDONLY, FS_O_WRONLY, FS_SEEK_SET, fs_close, fs_fstat, fs_lseek, fs_open, fs_pread,
    fs_read, fs_readdir, fs_write,
};
use mududb::types::data_value::DataValue;

fn no_params() -> SQLParamValue {
    SQLParamValue::from_vec(vec![])
}

fn text(s: &str) -> DataValue {
    DataValue::from_string(s.to_string())
}

#[test]
fn facade_scenario() {
    // 1. open (default worker)
    let db = Database::open().unwrap();

    // Harness cleanup: the standalone adapter keeps its SQLite state across
    // runs, so re-runs must start idempotently (not part of the canonical
    // scenario).
    db.command(&"DROP TABLE IF EXISTS t", &no_params()).unwrap();

    // 2. create table
    db.command(
        &"CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
        &no_params(),
    )
    .unwrap();

    // 3. two inserts with positional parameters, one affected row each
    let affected = db
        .command(
            &"INSERT INTO t (id, name) VALUES (?, ?)",
            &SQLParamValue::from_vec(vec![DataValue::from_i32(1), text("alice")]),
        )
        .unwrap();
    assert_eq!(affected, 1);
    let affected = db
        .command(
            &"INSERT INTO t (id, name) VALUES (?, ?)",
            &SQLParamValue::from_vec(vec![DataValue::from_i32(2), text("bob")]),
        )
        .unwrap();
    assert_eq!(affected, 1);

    // 4. query: columns, values by index and by name, eof
    let mut rs = db
        .query(&"SELECT id, name FROM t ORDER BY id", &no_params())
        .unwrap();
    assert_eq!(rs.column_count(), 2);
    assert_eq!(rs.column_name(0).unwrap(), "id");
    assert_eq!(rs.column_name(1).unwrap(), "name");
    assert_eq!(rs.find_column("name").unwrap(), 1);

    assert!(rs.next());
    let row = rs.current_row().unwrap();
    assert_eq!(row.value(0).unwrap().expect_i32(), &1);
    assert_eq!(
        row.value_by_name("name").unwrap().expect_string().as_str(),
        "alice"
    );
    assert!(!row.is_null(0).unwrap());

    assert!(rs.next());
    let row = rs.current_row().unwrap();
    assert_eq!(row.value_by_name("id").unwrap().expect_i32(), &2);
    assert_eq!(row.value(1).unwrap().expect_string().as_str(), "bob");

    assert!(!rs.next());
    assert!(rs.eof());

    // 5. update with a named parameter, one affected row
    let affected = db
        .command(
            &"UPDATE t SET name = :name WHERE id = :id",
            &SQLParamValue::from_vec_named(
                vec![text("carol"), DataValue::from_i32(2)],
                vec!["name".to_string(), "id".to_string()],
            ),
        )
        .unwrap();
    assert_eq!(affected, 1);
    let mut rs = db
        .query(&"SELECT name FROM t WHERE id = 2", &no_params())
        .unwrap();
    assert!(rs.next());
    assert_eq!(
        rs.current_row()
            .unwrap()
            .value(0)
            .unwrap()
            .expect_string()
            .as_str(),
        "carol"
    );

    // 6. fs roundtrip: write "hello", seek back, read/pread, fstat, readdir
    let session = db.id();
    let fs_oid = 7;
    let fd = fs_open(session, fs_oid, "docs/hello.txt", FS_O_WRONLY).unwrap();
    let written = fs_write(session, fd, b"hello").unwrap();
    assert_eq!(written, 5);
    fs_close(session, fd).unwrap();

    let fd = fs_open(session, fs_oid, "docs/hello.txt", FS_O_RDONLY).unwrap();
    let pos = fs_lseek(session, fd, 0, FS_SEEK_SET).unwrap();
    assert_eq!(pos, 0);
    let content = fs_read(session, fd, 5).unwrap();
    assert_eq!(content, b"hello");
    let slice = fs_pread(session, fd, 1, 3).unwrap();
    assert_eq!(slice, b"ell");
    let stat = fs_fstat(session, fd).unwrap();
    assert_eq!(stat.length, 5);
    fs_close(session, fd).unwrap();

    let entries = fs_readdir(session, fs_oid, "docs").unwrap();
    assert!(entries.iter().any(|e| e.name == "hello.txt" && !e.is_dir));

    // 7. close
    db.close().unwrap();
}
