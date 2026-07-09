#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::database::context::Context;
    use crate::database::db_conn::DBConnSync;
    use crate::database::entity::Entity;
    use crate::database::result_set::ResultSet;
    use crate::database::sql_params::SQLParams;
    use crate::database::sql_stmt::SQLStmt;
    use crate::tuple::tuple_field_desc::TupleFieldDesc;
    use crate::tuple::tuple_value::TupleValue;
    use mudu::common::id::OID;
    use mudu::common::result::RS;
    use mudu_sys::sync::SMutex;
    use std::sync::Arc;

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
            Ok(rows.remove(0))
        }
    }

    struct MockDBConn {
        query_result: Option<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)>,
        command_result: RS<u64>,
        last_sql: SMutex<Option<String>>,
    }

    impl MockDBConn {
        fn with_query(rows: Vec<TupleValue>) -> Self {
            let desc = i32::tuple_desc().clone();
            Self {
                query_result: Some((Arc::new(MockResultSet::new(rows)), Arc::new(desc))),
                command_result: Ok(0),
                last_sql: SMutex::new(None),
            }
        }

        fn with_command_result(result: RS<u64>) -> Self {
            Self {
                query_result: None,
                command_result: result,
                last_sql: SMutex::new(None),
            }
        }
    }

    impl DBConnSync for MockDBConn {
        fn exec_silent(&self, _sql_text: &str) -> RS<()> {
            Ok(())
        }

        fn begin_tx(&self) -> RS<OID> {
            Ok(0u128)
        }

        fn rollback_tx(&self) -> RS<()> {
            Ok(())
        }

        fn commit_tx(&self) -> RS<()> {
            Ok(())
        }

        fn query(
            &self,
            sql: &dyn SQLStmt,
            _param: &dyn SQLParams,
        ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
            *self.last_sql.lock().unwrap() = Some(sql.to_string());
            self.query_result
                .as_ref()
                .map(|(rs, desc)| (Arc::clone(rs), Arc::clone(desc)))
                .ok_or_else(|| {
                    mudu::mudu_error!(mudu::error::ErrorCode::Database, "no query result")
                })
        }

        fn command(&self, sql: &dyn SQLStmt, _param: &dyn SQLParams) -> RS<u64> {
            *self.last_sql.lock().unwrap() = Some(sql.to_string());
            self.command_result.clone()
        }

        fn batch(&self, _sql: &dyn SQLStmt, _param: &dyn SQLParams) -> RS<u64> {
            Ok(0)
        }
    }

    #[test]
    fn context_new_and_db_conn() {
        let conn: Arc<dyn DBConnSync> = Arc::new(MockDBConn::with_query(vec![]));
        let ctx = Context::new(conn.clone());
        let returned = ctx.db_conn();
        assert!(std::ptr::addr_eq(
            Arc::as_ptr(&returned) as *const (),
            Arc::as_ptr(&conn) as *const ()
        ));
    }

    #[test]
    fn context_query_returns_record_set() {
        let value = TupleValue::from(vec![mudu_type::data_value::DataValue::from_i32(42)]);
        let conn = Arc::new(MockDBConn::with_query(vec![value]));
        let ctx = Context::new(conn);
        let record_set = ctx.query::<i32>(&"SELECT 1", &()).unwrap();
        let record = record_set.next_record().unwrap();
        assert_eq!(record, Some(42));
    }

    #[test]
    fn context_command_returns_affected_rows() {
        let conn = Arc::new(MockDBConn::with_command_result(Ok(3)));
        let ctx = Context::new(conn);
        let affected = ctx.command(&"INSERT INTO t VALUES (1)", &()).unwrap();
        assert_eq!(affected, 3);
    }

    #[test]
    fn context_command_propagates_errors() {
        let conn = Arc::new(MockDBConn::with_command_result(Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Database,
            "boom"
        ))));
        let ctx = Context::new(conn);
        let err = ctx.command(&"INSERT INTO t VALUES (1)", &()).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Database);
    }

    #[test]
    fn context_query_propagates_missing_result_error() {
        let conn = Arc::new(MockDBConn::with_command_result(Ok(0)));
        let ctx = Context::new(conn);
        let err = ctx.query::<i32>(&"SELECT 1", &()).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Database);
    }

    #[test]
    fn context_exposes_db_conn_methods() {
        let conn = Arc::new(MockDBConn::with_query(vec![]));
        let ctx = Context::new(conn);
        let db_conn = ctx.db_conn();
        db_conn.exec_silent("VACUUM").unwrap();
        let tx = db_conn.begin_tx().unwrap();
        db_conn.rollback_tx().unwrap();
        let _ = db_conn.begin_tx().unwrap();
        db_conn.commit_tx().unwrap();
        assert_eq!(db_conn.batch(&"SELECT 1", &()).unwrap(), 0);
        assert_eq!(tx, 0u128);
    }
}
