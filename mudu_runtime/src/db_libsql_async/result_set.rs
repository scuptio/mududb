use async_trait::async_trait;
use libsql::{Row, Rows};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_sys::sync::SMutex as StdMutex;
use mudu_sys::sync::async_::AMutex;
use mudu_type::data_value::DataValue;
use mudu_type::type_family::TypeFamily;
use std::sync::Arc;
use tracing::debug;

pub trait ResultSetLease: Send + Sync {
    fn release(self: Box<Self>);
}

pub struct LibSQLAsyncResultSet {
    inner: Arc<ResultSetInner>,
}

pub struct ResultSetInner {
    row: AMutex<Rows>,
    tuple_desc: Arc<TupleFieldDesc>,
    lease: StdMutex<Option<Box<dyn ResultSetLease>>>,
}

impl LibSQLAsyncResultSet {
    pub fn new(
        rows: Rows,
        desc: Arc<TupleFieldDesc>,
        lease: Option<Box<dyn ResultSetLease>>,
    ) -> LibSQLAsyncResultSet {
        let inner = ResultSetInner::new(rows, desc, lease);
        Self {
            inner: Arc::new(inner),
        }
    }
}

#[async_trait]
impl ResultSetAsync for LibSQLAsyncResultSet {
    async fn next(&self) -> RS<Option<TupleValue>> {
        self.inner.async_next().await
    }

    fn desc(&self) -> &TupleFieldDesc {
        self.inner.tuple_desc.as_ref()
    }
}

impl ResultSetInner {
    fn new(
        row: Rows,
        tuple_desc: Arc<TupleFieldDesc>,
        lease: Option<Box<dyn ResultSetLease>>,
    ) -> ResultSetInner {
        Self {
            row: AMutex::new(row),
            tuple_desc,
            lease: StdMutex::new(lease),
        }
    }

    async fn async_next(&self) -> RS<Option<TupleValue>> {
        let mut guard = self.row.lock().await;
        let opt_row = guard
            .next()
            .await
            .map_err(|e| mudu_error!(ErrorCode::Database, "query result next", e))?;
        match opt_row {
            Some(row) => {
                let items = libsql_db_row_to_tuple_item(row, self.tuple_desc.fields())?;
                Ok(Some(items))
            }
            None => {
                self.release_lease();
                Ok(None)
            }
        }
    }

    fn release_lease(&self) {
        if let Ok(mut guard) = self.lease.lock()
            && let Some(lease) = guard.take()
        {
            lease.release();
        }
    }
}

impl Drop for ResultSetInner {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.lease.lock()
            && let Some(lease) = guard.take()
        {
            lease.release();
        }
    }
}

fn libsql_db_row_to_tuple_item(row: Row, item_desc: &[DatumDesc]) -> RS<TupleValue> {
    let mut vec = vec![];
    if row.column_count() as usize != item_desc.len() {
        return Err(mudu_error!(
            ErrorCode::FatalInternal,
            "column count mismatch"
        ));
    }
    for (i, desc) in item_desc.iter().enumerate() {
        let n = i as i32;
        let raw = row.get_value(n).unwrap();
        debug!("col={}, name={:?}, raw={:?}", n, row.column_name(n), raw);
        let internal = match desc.type_family() {
            TypeFamily::I32 => {
                let val = row.get::<i32>(n).map_err(|e| {
                    mudu_error!(ErrorCode::Database, "libsql db get item of row error", e)
                })?;
                DataValue::from_i32(val)
            }
            TypeFamily::I64 => {
                let val = row.get::<i64>(n).map_err(|e| {
                    mudu_error!(ErrorCode::Database, "libsql db get item of row error", e)
                })?;
                DataValue::from_i64(val)
            }
            TypeFamily::U128 => {
                let val = row.get::<String>(n).map_err(|e| {
                    mudu_error!(ErrorCode::Database, "libsql db get item of row error", e)
                })?;
                let val = val.parse::<u128>().map_err(|e| {
                    mudu_error!(ErrorCode::Database, "libsql db oid parse error", e)
                })?;
                DataValue::from_u128(val)
            }
            TypeFamily::I128 => {
                let val = row.get::<String>(n).map_err(|e| {
                    mudu_error!(ErrorCode::Database, "libsql db get item of row error", e)
                })?;
                let val = val.parse::<i128>().map_err(|e| {
                    mudu_error!(ErrorCode::Database, "libsql db i128 parse error", e)
                })?;
                DataValue::from_i128(val)
            }
            TypeFamily::F32 => {
                let val = row.get::<f64>(n).map_err(|e| {
                    mudu_error!(ErrorCode::Database, "libsql db get item of row error", e)
                })?;
                DataValue::from_f64(val)
            }
            TypeFamily::F64 => {
                let val = row.get::<f64>(n).map_err(|_e| {
                    mudu_error!(ErrorCode::Database, "libsql db get item of row error")
                })?;
                DataValue::from_f64(val)
            }
            TypeFamily::String => {
                let val = row
                    .get::<String>(n)
                    .map_err(|e| mudu_error!(ErrorCode::Database, "get item of row error", e))?;
                DataValue::from_string(val)
            }
            _ => {
                return Err(mudu_error!(
                    ErrorCode::InvalidType,
                    format!(
                        "libsql unsupported type in async result conversion: {:?}",
                        desc
                    )
                ));
            }
        };

        vec.push(internal)
    }
    Ok(TupleValue::from(vec))
}

// libsql/SQLite FFI is not available under Miri, so exclude this module.
#[cfg(all(test, not(miri)))]
mod tests {
    use super::*;
    use mudu_contract::tuple::datum_desc::DatumDesc;
    use mudu_type::data_type::DataType;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tempfile::tempdir;

    struct FlagLease(Arc<AtomicBool>);

    impl ResultSetLease for FlagLease {
        fn release(self: Box<Self>) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    fn make_desc(fields: Vec<DatumDesc>) -> Arc<TupleFieldDesc> {
        Arc::new(TupleFieldDesc::new(fields))
    }

    fn field(name: &str, id: TypeFamily) -> DatumDesc {
        DatumDesc::new(name.to_string(), DataType::new_no_param(id))
    }

    async fn open_conn() -> (libsql::Connection, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = libsql::Builder::new_local(db_path).build().await.unwrap();
        let conn = db.connect().unwrap();
        (conn, dir)
    }

    #[tokio::test]
    async fn next_converts_all_supported_types() {
        let (conn, _dir) = open_conn().await;
        conn.execute_batch(
            "CREATE TABLE t(
                i32_col INTEGER,
                i64_col BIGINT,
                u128_col TEXT,
                i128_col TEXT,
                f32_col REAL,
                f64_col REAL,
                string_col TEXT
            );
            INSERT INTO t VALUES (1, 2, '42', '-42', 1.5, 2.5, 'hello');",
        )
        .await
        .unwrap();

        let rows = conn.query("SELECT * FROM t", ()).await.unwrap();
        let desc = make_desc(vec![
            field("i32_col", TypeFamily::I32),
            field("i64_col", TypeFamily::I64),
            field("u128_col", TypeFamily::U128),
            field("i128_col", TypeFamily::I128),
            field("f32_col", TypeFamily::F32),
            field("f64_col", TypeFamily::F64),
            field("string_col", TypeFamily::String),
        ]);
        let rs = LibSQLAsyncResultSet::new(rows, desc, None);

        let row = rs.next().await.unwrap().unwrap();
        let vals = row.values();
        assert_eq!(vals[0].to_i32(), 1);
        assert_eq!(vals[1].to_i64(), 2);
        assert_eq!(vals[2].to_oid(), 42);
        assert_eq!(vals[3].to_i128(), -42);
        assert_eq!(vals[4].to_f64(), 1.5);
        assert_eq!(vals[5].to_f64(), 2.5);
        assert_eq!(vals[6].as_string().unwrap(), "hello");
    }

    #[tokio::test]
    async fn next_returns_none_when_exhausted_and_releases_lease() {
        let (conn, _dir) = open_conn().await;
        conn.execute_batch("CREATE TABLE t(a INTEGER); INSERT INTO t VALUES (1);")
            .await
            .unwrap();

        let rows = conn.query("SELECT * FROM t", ()).await.unwrap();
        let desc = make_desc(vec![field("a", TypeFamily::I32)]);
        let released = Arc::new(AtomicBool::new(false));
        let lease = Box::new(FlagLease(released.clone()));
        let rs = LibSQLAsyncResultSet::new(rows, desc, Some(lease));

        let row = rs.next().await.unwrap().unwrap();
        assert_eq!(row.values()[0].to_i32(), 1);
        assert!(!released.load(Ordering::SeqCst));

        let opt = rs.next().await.unwrap();
        assert!(opt.is_none());
        assert!(released.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn lease_released_on_drop_if_rows_not_consumed() {
        let (conn, _dir) = open_conn().await;
        conn.execute_batch("CREATE TABLE t(a INTEGER); INSERT INTO t VALUES (1);")
            .await
            .unwrap();

        let rows = conn.query("SELECT * FROM t", ()).await.unwrap();
        let desc = make_desc(vec![field("a", TypeFamily::I32)]);
        let released = Arc::new(AtomicBool::new(false));
        let lease = Box::new(FlagLease(released.clone()));
        let rs = LibSQLAsyncResultSet::new(rows, desc, Some(lease));
        drop(rs);
        assert!(released.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn column_count_mismatch_returns_fatal_internal() {
        let (conn, _dir) = open_conn().await;
        conn.execute_batch("CREATE TABLE t(a INTEGER, b INTEGER); INSERT INTO t VALUES (1, 2);")
            .await
            .unwrap();

        let rows = conn.query("SELECT * FROM t", ()).await.unwrap();
        let desc = make_desc(vec![field("a", TypeFamily::I32)]);
        let rs = LibSQLAsyncResultSet::new(rows, desc, None);

        let err = rs.next().await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::FatalInternal);
        assert!(err.message().contains("column count mismatch"));
    }

    #[tokio::test]
    async fn unsupported_blob_type_returns_invalid_type() {
        let (conn, _dir) = open_conn().await;
        conn.execute_batch("CREATE TABLE t(a BLOB); INSERT INTO t VALUES (x'0102');")
            .await
            .unwrap();

        let rows = conn.query("SELECT * FROM t", ()).await.unwrap();
        let desc = make_desc(vec![DatumDesc::new(
            "a".to_string(),
            DataType::new_no_param(TypeFamily::Binary),
        )]);
        let rs = LibSQLAsyncResultSet::new(rows, desc, None);

        let err = rs.next().await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidType);
        assert!(err.message().contains("unsupported type"));
    }

    #[tokio::test]
    async fn desc_returns_the_descriptor_passed_to_new() {
        let (conn, _dir) = open_conn().await;
        conn.execute_batch("CREATE TABLE t(a INTEGER);")
            .await
            .unwrap();

        let rows = conn.query("SELECT * FROM t", ()).await.unwrap();
        let desc = make_desc(vec![field("a", TypeFamily::I32)]);
        let rs = LibSQLAsyncResultSet::new(rows, desc.clone(), None);

        assert_eq!(rs.desc().fields().len(), 1);
        assert_eq!(rs.desc().fields()[0].name(), "a");
        assert_eq!(rs.desc().fields()[0].type_family(), TypeFamily::I32);
    }

    #[tokio::test]
    async fn multiple_rows_returned_sequentially() {
        let (conn, _dir) = open_conn().await;
        conn.execute_batch(
            "CREATE TABLE t(a INTEGER, b TEXT);
            INSERT INTO t VALUES (1, 'one');
            INSERT INTO t VALUES (2, 'two');
            INSERT INTO t VALUES (3, 'three');",
        )
        .await
        .unwrap();

        let rows = conn.query("SELECT * FROM t ORDER BY a", ()).await.unwrap();
        let desc = make_desc(vec![
            field("a", TypeFamily::I32),
            field("b", TypeFamily::String),
        ]);
        let rs = LibSQLAsyncResultSet::new(rows, desc, None);

        let expected = [(1, "one"), (2, "two"), (3, "three")];
        for (exp_a, exp_b) in expected {
            let row = rs.next().await.unwrap().unwrap();
            let vals = row.values();
            assert_eq!(vals[0].to_i32(), exp_a);
            assert_eq!(vals[1].as_string().unwrap(), exp_b);
        }
        assert!(rs.next().await.unwrap().is_none());
    }
}

#[cfg(test)]
#[path = "result_set_test.rs"]
mod result_set_test;
