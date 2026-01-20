use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::sql::Context;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;


/// Execute a SQL query with parameters
pub fn query_internal(query_in: &[u8]) -> Vec<u8> {
    let r = _query_internal(query_in);
    mudu_binding::system::query_invoke::serialize_query_result(r)
}

fn _query_internal(query_in: &[u8]) -> RS<(ResultBatch, TupleFieldDesc)> {
    let (oid, stmt, param) = mudu_binding::system::query_invoke::deserialize_query_param(query_in)?;
    let context = get_context(oid)?;
    let (rs, desc) = context.query_raw(stmt.as_ref(), param.as_ref())?;
    let batch = ResultBatch::from_result_set(oid, rs.as_ref())?;
    Ok((batch, desc.as_ref().clone()))
}


/// Fetch the next row from a result cursor
pub fn fetch_internal(_: &[u8]) -> Vec<u8> {
    Default::default()
}

/// Execute a SQL command with parameters
pub fn command_internal(command_in: &[u8]) -> Vec<u8> {
    let r = _command_internal(command_in);
    mudu_binding::system::command_invoke::serialize_command_result(r)
}
fn _command_internal(command_in: &[u8]) -> RS<u64> {
    let (oid, stmt, param) = mudu_binding::system::command_invoke::deserialize_command_param(command_in)?;
    let context = get_context(oid)?;
    let r = context.command(stmt.as_ref(), param.as_ref())?;
    Ok(r)
}


/// Execute a SQL query with parameters
pub async fn async_query_internal(query_in: Vec<u8>) -> Vec<u8> {
    let r = _async_query_internal(query_in).await;
    mudu_binding::system::query_invoke::serialize_query_result(r)
}

async fn _async_query_internal(query_in: Vec<u8>) -> RS<(ResultBatch, TupleFieldDesc)> {
    let (oid, stmt, param) = mudu_binding::system::query_invoke::deserialize_query_param(&query_in)?;
    let context = get_context(oid)?;
    let rs = context.query_raw_async(stmt, param).await?;
    let batch = ResultBatch::from_result_set_async(oid, rs.as_ref()).await?;
    Ok((batch, rs.desc().clone()))
}


/// Fetch the next row from a result cursor
pub async fn async_fetch_internal(_: Vec<u8>) -> Vec<u8> {
    Default::default()
}

/// Execute a SQL command with parameters
pub async fn async_command_internal(command_in: Vec<u8>) -> Vec<u8> {
    let r = _async_command_internal(command_in).await;
    mudu_binding::system::command_invoke::serialize_command_result(r)
}
async fn _async_command_internal(command_in: Vec<u8>) -> RS<u64> {
    let (oid, stmt, param) = mudu_binding::system::command_invoke::deserialize_command_param(&command_in)?;
    let context = get_context(oid)?;
    let r = context.command_async(stmt, param).await?;
    Ok(r)
}

fn get_context(xid: XID) -> RS<Context> {
    let opt = Context::context(xid);
    match opt {
        Some(ctx) => {
            Ok(ctx)
        },
        None => {
            Err(m_error!(EC::NoneErr, format!("no such transaction id: {}", xid)))
        }
    }
}