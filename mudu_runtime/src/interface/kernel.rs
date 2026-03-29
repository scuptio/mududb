use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::sql::Context;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_kernel::server_ur::worker_local::WorkerLocalRef;

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

pub fn batch_internal(batch_in: &[u8]) -> Vec<u8> {
    let r = _batch_internal(batch_in);
    mudu_binding::system::command_invoke::serialize_command_result(r)
}

fn _command_internal(command_in: &[u8]) -> RS<u64> {
    let (oid, stmt, param) =
        mudu_binding::system::command_invoke::deserialize_command_param(command_in)?;
    let context = get_context(oid)?;
    let r = context.command(stmt.as_ref(), param.as_ref())?;
    Ok(r)
}

fn _batch_internal(batch_in: &[u8]) -> RS<u64> {
    let (oid, stmt, param) = mudu_binding::system::command_invoke::deserialize_command_param(batch_in)?;
    let context = get_context(oid)?;
    context.batch(stmt.as_ref(), param.as_ref())
}

/// Execute a SQL query with parameters
pub async fn async_query_internal(query_in: Vec<u8>) -> Vec<u8> {
    let r = _async_query_internal(query_in).await;
    mudu_binding::system::query_invoke::serialize_query_result(r)
}

async fn _async_query_internal(query_in: Vec<u8>) -> RS<(ResultBatch, TupleFieldDesc)> {
    let (oid, stmt, param) =
        mudu_binding::system::query_invoke::deserialize_query_param(&query_in)?;
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

pub async fn async_batch_internal(batch_in: Vec<u8>) -> Vec<u8> {
    let r = _async_batch_internal(batch_in).await;
    mudu_binding::system::command_invoke::serialize_command_result(r)
}

async fn _async_command_internal(command_in: Vec<u8>) -> RS<u64> {
    let (oid, stmt, param) =
        mudu_binding::system::command_invoke::deserialize_command_param(&command_in)?;
    let context = get_context(oid)?;
    let r = context.command_async(stmt, param).await?;
    Ok(r)
}

async fn _async_batch_internal(batch_in: Vec<u8>) -> RS<u64> {
    let (oid, stmt, param) = mudu_binding::system::command_invoke::deserialize_command_param(&batch_in)?;
    let context = get_context(oid)?;
    context.batch_async(stmt, param).await
}

fn get_context(oid: OID) -> RS<Context> {
    let opt = Context::context(oid);
    match opt {
        Some(ctx) => Ok(ctx),
        None => Err(m_error!(
            EC::NoneErr,
            format!("no such session id: {}", oid)
        )),
    }
}

pub fn open_internal_with_worker_local(
    open_in: &[u8],
    worker_local: Option<&WorkerLocalRef>,
) -> RS<Vec<u8>> {
    let open_argv = sys_interface::host::deserialize_open_param(open_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let opened = worker_local.open_argv(open_argv.worker_oid())?;
    Ok(sys_interface::host::serialize_open_result(opened))
}

pub fn close_internal_with_worker_local(
    close_in: &[u8],
    worker_local: Option<&WorkerLocalRef>,
) -> RS<Vec<u8>> {
    let session_id: OID = sys_interface::host::deserialize_close_param(close_in)?;
    let worker_local = require_worker_local(worker_local)?;
    worker_local.close(session_id)?;
    Ok(sys_interface::host::serialize_close_result())
}

pub fn get_internal(get_in: &[u8]) -> Vec<u8> {
    get_internal_with_worker_local(get_in, None)
        .unwrap_or_else(|e| panic!("worker-local get is not available: {}", e))
}

pub fn get_internal_with_worker_local(
    get_in: &[u8],
    worker_local: Option<&WorkerLocalRef>,
) -> RS<Vec<u8>> {
    let result =
        sys_interface::host::deserialize_session_get_param(get_in).and_then(|(session_id, key)| {
            let worker_local = require_worker_local(worker_local)?;
            worker_local.get(session_id, &key)
        });
    Ok(sys_interface::host::serialize_get_result(
        result?.as_deref(),
    ))
}

pub fn put_internal(put_in: &[u8]) -> Vec<u8> {
    put_internal_with_worker_local(put_in, None)
        .unwrap_or_else(|e| panic!("worker-local put is not available: {}", e))
}

pub fn put_internal_with_worker_local(
    put_in: &[u8],
    worker_local: Option<&WorkerLocalRef>,
) -> RS<Vec<u8>> {
    let result = sys_interface::host::deserialize_session_put_param(put_in).and_then(
        |(session_id, key, value)| {
            let worker_local = require_worker_local(worker_local)?;
            worker_local.put(session_id, key, value)
        },
    );
    result?;
    Ok(sys_interface::host::serialize_put_result())
}

pub fn range_internal(range_in: &[u8]) -> Vec<u8> {
    range_internal_with_worker_local(range_in, None)
        .unwrap_or_else(|e| panic!("worker-local range is not available: {}", e))
}

pub fn range_internal_with_worker_local(
    range_in: &[u8],
    worker_local: Option<&WorkerLocalRef>,
) -> RS<Vec<u8>> {
    let result = sys_interface::host::deserialize_session_range_param(range_in).and_then(
        |(session_id, start, end)| {
            let worker_local = require_worker_local(worker_local)?;
            Ok(worker_local
                .range(session_id, &start, &end)?
                .into_iter()
                .map(|item| (item.key, item.value))
                .collect::<Vec<_>>())
        },
    );
    Ok(sys_interface::host::serialize_range_result(&result?))
}

pub async fn async_get_internal(get_in: Vec<u8>) -> Vec<u8> {
    get_internal(&get_in)
}

pub async fn async_get_internal_with_worker_local(
    get_in: Vec<u8>,
    worker_local: Option<&WorkerLocalRef>,
) -> Vec<u8> {
    get_internal_with_worker_local(&get_in, worker_local)
        .unwrap_or_else(|e| panic!("worker-local get is not available: {}", e))
}

pub async fn async_open_internal_with_worker_local(
    open_in: Vec<u8>,
    worker_local: Option<&WorkerLocalRef>,
) -> Vec<u8> {
    open_internal_with_worker_local(&open_in, worker_local)
        .unwrap_or_else(|e| panic!("worker-local open is not available: {}", e))
}

pub async fn async_close_internal_with_worker_local(
    close_in: Vec<u8>,
    worker_local: Option<&WorkerLocalRef>,
) -> Vec<u8> {
    close_internal_with_worker_local(&close_in, worker_local)
        .unwrap_or_else(|e| panic!("worker-local close is not available: {}", e))
}

pub async fn async_put_internal(put_in: Vec<u8>) -> Vec<u8> {
    put_internal(&put_in)
}

pub async fn async_put_internal_with_worker_local(
    put_in: Vec<u8>,
    worker_local: Option<&WorkerLocalRef>,
) -> Vec<u8> {
    put_internal_with_worker_local(&put_in, worker_local)
        .unwrap_or_else(|e| panic!("worker-local put is not available: {}", e))
}

pub async fn async_range_internal(range_in: Vec<u8>) -> Vec<u8> {
    range_internal(&range_in)
}

pub async fn async_range_internal_with_worker_local(
    range_in: Vec<u8>,
    worker_local: Option<&WorkerLocalRef>,
) -> Vec<u8> {
    range_internal_with_worker_local(&range_in, worker_local)
        .unwrap_or_else(|e| panic!("worker-local range is not available: {}", e))
}

fn require_worker_local(worker_local: Option<&WorkerLocalRef>) -> RS<&WorkerLocalRef> {
    worker_local.ok_or_else(|| {
        m_error!(
            EC::NotImplemented,
            "worker local interface is not configured for this runtime path"
        )
    })
}

pub fn empty_query_internal(_: &[u8]) -> Vec<u8> {
    // The io_uring KV-only architecture intentionally leaves SQL syscalls empty.
    Vec::new()
}

pub fn empty_command_internal(_: &[u8]) -> Vec<u8> {
    // The io_uring KV-only architecture intentionally leaves SQL syscalls empty.
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kv_syscalls_require_worker_local() {
        let get = sys_interface::host::serialize_session_get_param(1, b"alpha");
        let err = get_internal_with_worker_local(&get, None).unwrap_err();
        assert!(
            err.to_string()
                .contains("worker local interface is not configured")
        );
    }
}
