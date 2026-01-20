use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::result_set::ResultSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_value::TupleValue;
use std::sync::{Arc, Mutex};

#[allow(unused)]
pub fn invoke_host_command<F>(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
    f: F,
) -> RS<u64>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = mudu_binding::system::command_invoke::serialize_command_param(xid, sql, params)?;
    let result = f(param_binary)?;
    let affected_rows = mudu_binding::system::command_invoke::deserialize_command_result(&result)?;
    Ok(affected_rows)
}

#[allow(unused)]
pub fn invoke_host_query<R: Entity, F>(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
    f: F,
) -> RS<RecordSet<R>>
where
    F: Fn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = mudu_binding::system::query_invoke::serialize_query_dyn_param(xid, sql, params)?;
    let result = f(param_binary)?;
    let (result_batch, tuple_desc) = mudu_binding::system::query_invoke::deserialize_query_result(&result)?;
    let record_set = RecordSet::<R>::new(
        Arc::new(ResultSetWrapper::new(result_batch)), Arc::new(tuple_desc));
    Ok(record_set)
}

#[allow(unused)]
pub async fn async_invoke_host_command<F>(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
    f: F,
) -> RS<u64>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = mudu_binding::system::command_invoke::serialize_command_param(xid, sql, params)?;
    let result = f(param_binary).await?;
    let affected_rows = mudu_binding::system::command_invoke::deserialize_command_result(&result)?;
    Ok(affected_rows)
}

#[allow(unused)]
pub async fn async_invoke_host_query<R: Entity, F>(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
    f: F,
) -> RS<RecordSet<R>>
where
    F: AsyncFn(Vec<u8>) -> RS<Vec<u8>>,
{
    let param_binary = mudu_binding::system::query_invoke::serialize_query_dyn_param(xid, sql, params)?;
    let result = f(param_binary).await?;
    let (result_batch, tuple_desc) = mudu_binding::system::query_invoke::deserialize_query_result(&result)?;
    let record_set = RecordSet::<R>::new(
        Arc::new(ResultSetWrapper::new(result_batch)), Arc::new(tuple_desc));
    Ok(record_set)
}

pub struct ResultSetWrapper {
    batch: Mutex<ResultBatch>,
}

impl ResultSetWrapper {
    pub fn new(batch: ResultBatch) -> ResultSetWrapper {
        ResultSetWrapper { batch: Mutex::new(batch) }
    }
}

impl ResultSet for ResultSetWrapper {
    fn next(&self) -> RS<Option<TupleValue>> {
        let mut batch = self.batch.lock().unwrap();
        let t = batch.mut_rows().pop();
        Ok(t)
    }
}