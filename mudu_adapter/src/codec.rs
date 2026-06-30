//! Adapter helper functions used by generated bindings to forward host calls.

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

/// Invokes a synchronous host command closure and returns the affected row count.
pub fn invoke_host_command<F>(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams, f: F) -> RS<u64>
where
    F: Fn(OID, &dyn SQLStmt, &dyn SQLParams) -> RS<u64>,
{
    f(oid, sql, params)
}

/// Invokes a synchronous host query closure and returns the resulting record set.
pub fn invoke_host_query<R: Entity, F>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
    f: F,
) -> RS<RecordSet<R>>
where
    F: Fn(OID, &dyn SQLStmt, &dyn SQLParams) -> RS<RecordSet<R>>,
{
    f(oid, sql, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mudu_contract::database::result_set::ResultSet;
    use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
    use std::sync::Arc;

    struct EmptyResultSet;

    impl ResultSet for EmptyResultSet {
        fn next(&self) -> RS<Option<mudu_contract::tuple::tuple_value::TupleValue>> {
            Ok(None)
        }
    }

    #[test]
    fn invoke_host_command_forwards_args_and_returns_result() {
        let oid = 42u128;
        let sql = String::from("INSERT INTO t VALUES (?)");
        let result = invoke_host_command(oid, &sql, &(), |got_oid, got_sql, got_params| {
            assert_eq!(got_oid, oid);
            assert_eq!(got_sql.to_sql_string(), sql);
            assert_eq!(got_params.size(), 0);
            Ok(1)
        });
        assert_eq!(result, Ok(1));
    }

    #[test]
    fn invoke_host_query_forwards_args_and_returns_result() {
        let oid = 7u128;
        let sql = String::from("SELECT 1");
        let result = invoke_host_query(oid, &sql, &(), |got_oid, got_sql, got_params| {
            assert_eq!(got_oid, oid);
            assert_eq!(got_sql.to_sql_string(), sql);
            assert_eq!(got_params.size(), 0);
            Ok(RecordSet::<i32>::new(
                Arc::new(EmptyResultSet),
                Arc::new(TupleFieldDesc::new(vec![])),
            ))
        });
        assert!(result.is_ok());
    }
}
