use super::*;

/// Minimal sync connection returning a fixed row set, mirroring the
/// sys_interface fetch tests (`MockDBConnSync` in
/// `crates/common/sys_interface/src/api_impl/sync.rs`).
struct MockResultSet {
    rows: SMutex<Vec<Option<TupleValue>>>,
}

impl MockResultSet {
    fn new(rows: Vec<TupleValue>) -> Self {
        Self {
            rows: SMutex::new(rows.into_iter().map(Some).collect()),
        }
    }
}

impl ResultSet for MockResultSet {
    fn next(&self) -> RS<Option<TupleValue>> {
        let mut rows = self.rows.lock().unwrap();
        Ok(if rows.is_empty() {
            None
        } else {
            rows.remove(0)
        })
    }
}

struct MockDBConnSync {
    query_result: RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)>,
}

impl MockDBConnSync {
    fn with_query(rows: Vec<TupleValue>) -> Self {
        let desc = i32::tuple_desc().clone();
        Self {
            query_result: Ok((Arc::new(MockResultSet::new(rows)), Arc::new(desc))),
        }
    }
}

impl DBConnSync for MockDBConnSync {
    fn exec_silent(&self, _sql_text: &str) -> RS<()> {
        Ok(())
    }

    fn begin_tx(&self) -> RS<OID> {
        Ok(1)
    }

    fn rollback_tx(&self) -> RS<()> {
        Ok(())
    }

    fn commit_tx(&self) -> RS<()> {
        Ok(())
    }

    fn query(
        &self,
        _sql: &dyn SQLStmt,
        _param: &dyn SQLParams,
    ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
        self.query_result.clone()
    }

    fn command(&self, _sql: &dyn SQLStmt, _param: &dyn SQLParams) -> RS<u64> {
        Ok(0)
    }

    fn batch(&self, _sql: &dyn SQLStmt, _param: &dyn SQLParams) -> RS<u64> {
        Ok(0)
    }
}

fn query_context(oid: OID, rows: Vec<TupleValue>) -> Context {
    let conn = DBConn::Sync(Arc::new(MockDBConnSync::with_query(rows)));
    Context::create(oid, conn).unwrap()
}

fn fetch_cursor(oid: OID) -> Vec<u8> {
    encode_value(&UniOid::from(oid).to_value())
}

fn decode_fetch_result(bytes: &[u8]) -> UniResult<UniResultSet, UniError> {
    UniResult::from_value(&decode_value(bytes).unwrap()).unwrap()
}

fn first_i32_from_uni_result_set(rs: &UniResultSet) -> i32 {
    *rs.row_set[0].fields[0]
        .as_scalar()
        .unwrap()
        .as_i32()
        .unwrap()
}

#[test]
fn fetch_internal_drains_cached_rows() {
    // Mirrors sys_interface's `mudu_fetch_bytes_drains_cached_rows`:
    // rows cached on the context are drained by the first fetch; the
    // second fetch finds the cache cleared and reports eof with no rows.
    let oid = 9_100_001;
    let rows = vec![
        TupleValue::from(vec![DataValue::from_i32(10)]),
        TupleValue::from(vec![DataValue::from_i32(20)]),
    ];
    let ctx = query_context(oid, rows);
    let (rs, desc) = ctx.query_raw(&"SELECT 1", &()).unwrap();
    ctx.cache_result((rs, desc)).unwrap();

    let out = fetch_internal(&fetch_cursor(oid));
    let result_set = match decode_fetch_result(&out) {
        UniResult::Ok(rs) => rs,
        UniResult::Err(err) => panic!("unexpected error: {}", err.err_msg),
    };
    assert!(result_set.eof);
    assert_eq!(result_set.row_set.len(), 2);
    assert_eq!(first_i32_from_uni_result_set(&result_set), 10);
    // The response cursor round-trips the context oid.
    let cursor_oid = UniOid::from_value(&decode_value(&result_set.cursor).unwrap()).unwrap();
    assert_eq!(cursor_oid.to_oid(), oid);

    let out = fetch_internal(&fetch_cursor(oid));
    match decode_fetch_result(&out) {
        UniResult::Ok(rs) => {
            assert!(rs.eof);
            assert!(rs.row_set.is_empty());
        }
        UniResult::Err(err) => panic!("unexpected error: {}", err.err_msg),
    }

    Context::remove(oid);
}

#[test]
fn fetch_internal_after_query_reports_eof_with_no_rows() {
    // The query path drains the cached result set into the query
    // response itself, so a subsequent fetch returns an empty, eof
    // batch (sys_interface semantics).
    let oid = 9_100_002;
    let rows = vec![TupleValue::from(vec![DataValue::from_i32(42)])];
    let _ctx = query_context(oid, rows);

    let query_in =
        mudu_binding::system::query_invoke::serialize_query_dyn_param(oid, &"SELECT 1", &())
            .unwrap();
    let out = query_internal(&query_in);
    let (batch, _desc) =
        mudu_binding::system::query_invoke::deserialize_query_result(&out).unwrap();
    assert_eq!(batch.rows().len(), 1);
    assert!(batch.is_eof());

    let out = fetch_internal(&fetch_cursor(oid));
    match decode_fetch_result(&out) {
        UniResult::Ok(rs) => {
            assert!(rs.eof);
            assert!(rs.row_set.is_empty());
        }
        UniResult::Err(err) => panic!("unexpected error: {}", err.err_msg),
    }

    Context::remove(oid);
}

#[test]
fn fetch_internal_unknown_cursor_returns_uni_error() {
    let out = fetch_internal(&fetch_cursor(9_100_003));
    match decode_fetch_result(&out) {
        UniResult::Err(err) => {
            assert_eq!(err.err_code, ErrorCode::EntityNotFound.to_u32())
        }
        UniResult::Ok(_) => panic!("expected error"),
    }
}

#[test]
fn fetch_internal_malformed_cursor_returns_uni_error() {
    // 0xc1 is a reserved MessagePack marker: decoding it as UniOid fails.
    let out = fetch_internal(&[0xc1]);
    match decode_fetch_result(&out) {
        UniResult::Err(_) => {}
        UniResult::Ok(_) => panic!("expected error"),
    }
}

#[tokio::test]
async fn async_fetch_internal_drains_cached_rows() {
    let oid = 9_100_004;
    let rows = vec![TupleValue::from(vec![DataValue::from_i32(7)])];
    let ctx = query_context(oid, rows);
    let (rs, desc) = ctx.query_raw(&"SELECT 1", &()).unwrap();
    ctx.cache_result((rs, desc)).unwrap();

    let out = async_fetch_internal(fetch_cursor(oid)).await;
    match decode_fetch_result(&out) {
        UniResult::Ok(rs) => {
            assert!(rs.eof);
            assert_eq!(rs.row_set.len(), 1);
            assert_eq!(first_i32_from_uni_result_set(&rs), 7);
        }
        UniResult::Err(err) => panic!("unexpected error: {}", err.err_msg),
    }

    Context::remove(oid);
}

#[tokio::test]
async fn async_fetch_internal_unknown_cursor_returns_uni_error() {
    let out = async_fetch_internal(fetch_cursor(9_100_005)).await;
    match decode_fetch_result(&out) {
        UniResult::Err(err) => {
            assert_eq!(err.err_code, ErrorCode::EntityNotFound.to_u32())
        }
        UniResult::Ok(_) => panic!("expected error"),
    }
}
