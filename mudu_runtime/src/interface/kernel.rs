#![allow(missing_docs)]

use crate::async_utils::blocking::run_async;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::codec::handle_sys_session;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::sql::Context;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_kernel::server::worker_local::WorkerLocalRef;
use mudu_utils::task_trace;

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
    let (oid, stmt, param) =
        mudu_binding::system::command_invoke::deserialize_command_param(batch_in)?;
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
    let trace = task_trace!();
    trace.watch(
        "procedure.host.command.stage",
        "async_command_internal_start",
    );
    let r = _async_command_internal(command_in).await;
    trace.watch(
        "procedure.host.command.stage",
        if r.is_ok() {
            "async_command_internal_done"
        } else {
            "async_command_internal_error"
        },
    );
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
    context.command_async(stmt, param).await
}

async fn _async_batch_internal(batch_in: Vec<u8>) -> RS<u64> {
    let (oid, stmt, param) =
        mudu_binding::system::command_invoke::deserialize_command_param(&batch_in)?;
    let context = get_context(oid)?;
    context.batch_async(stmt, param).await
}

fn get_context(oid: OID) -> RS<Context> {
    let opt = Context::context(oid);
    match opt {
        Some(ctx) => Ok(ctx),
        None => Err(mudu_error!(
            ErrorCode::EntityNotFound,
            format!("no such session id: {}", oid)
        )),
    }
}

pub fn open_internal_with_worker_local(
    open_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> RS<Vec<u8>> {
    let open_argv = handle_sys_session::deserialize_open_param(open_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let worker_oid = open_argv.worker_oid();
    let opened = run_async(async move { worker_local.open_argv_async(worker_oid).await })??;
    Ok(handle_sys_session::serialize_open_result(opened))
}

pub fn close_internal_with_worker_local(
    close_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> RS<Vec<u8>> {
    let session_id: OID = handle_sys_session::deserialize_close_param(close_in)?;
    let worker_local = require_worker_local(worker_local)?;
    run_async(async move { worker_local.close_async(session_id).await })??;
    Ok(handle_sys_session::serialize_close_result())
}

pub fn get_internal(get_in: &[u8]) -> Vec<u8> {
    get_internal_with_worker_local(get_in, None)
        .unwrap_or_else(handle_sys_session::serialize_error_result)
}

pub fn get_internal_with_worker_local(
    get_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> RS<Vec<u8>> {
    let (session_id, key) = handle_sys_session::deserialize_session_get_param(get_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let result = run_async(async move { worker_local.get_async(session_id, &key).await })??;
    Ok(handle_sys_session::serialize_get_result(result.as_deref()))
}

pub fn put_internal(put_in: &[u8]) -> Vec<u8> {
    put_internal_with_worker_local(put_in, None)
        .unwrap_or_else(handle_sys_session::serialize_error_result)
}

pub fn put_internal_with_worker_local(
    put_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> RS<Vec<u8>> {
    let (session_id, key, value) = handle_sys_session::deserialize_session_put_param(put_in)?;
    let worker_local = require_worker_local(worker_local)?;
    run_async(async move { worker_local.put_async(session_id, key, value).await })??;
    Ok(handle_sys_session::serialize_put_result())
}

pub fn delete_internal(delete_in: &[u8]) -> Vec<u8> {
    delete_internal_with_worker_local(delete_in, None)
        .unwrap_or_else(handle_sys_session::serialize_error_result)
}

pub fn delete_internal_with_worker_local(
    delete_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> RS<Vec<u8>> {
    let (session_id, key) = handle_sys_session::deserialize_session_delete_param(delete_in)?;
    let worker_local = require_worker_local(worker_local)?;
    run_async(async move { worker_local.delete_async(session_id, &key).await })??;
    Ok(handle_sys_session::serialize_delete_result())
}

pub fn range_internal(range_in: &[u8]) -> Vec<u8> {
    range_internal_with_worker_local(range_in, None)
        .unwrap_or_else(handle_sys_session::serialize_error_result)
}

pub fn range_internal_with_worker_local(
    range_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> RS<Vec<u8>> {
    let (session_id, start, end) = handle_sys_session::deserialize_session_range_param(range_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let result =
        run_async(async move { worker_local.range_async(session_id, &start, &end).await })??;
    let result = result
        .into_iter()
        .map(|item| (item.key, item.value))
        .collect::<Vec<_>>();
    Ok(handle_sys_session::serialize_range_result(&result))
}

pub async fn async_get_internal(get_in: Vec<u8>) -> Vec<u8> {
    get_internal(&get_in)
}

pub async fn async_get_internal_with_worker_local(
    get_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    let result =
        handle_sys_session::deserialize_session_get_param(&get_in).and_then(|(session_id, key)| {
            let worker_local = require_worker_local(worker_local)?;
            Ok((worker_local, session_id, key))
        });
    match result {
        Ok((worker_local, session_id, key)) => {
            match worker_local.get_async(session_id, &key).await {
                Ok(value) => handle_sys_session::serialize_get_result(value.as_deref()),
                Err(err) => handle_sys_session::serialize_error_result(err),
            }
        }
        Err(err) => handle_sys_session::serialize_error_result(err),
    }
}

pub async fn async_open_internal_with_worker_local(
    open_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    let result = handle_sys_session::deserialize_open_param(&open_in).and_then(|open_argv| {
        let worker_local = require_worker_local(worker_local)?;
        Ok((open_argv, worker_local))
    });
    match result {
        Ok((open_argv, worker_local)) => {
            match worker_local.open_argv_async(open_argv.worker_oid()).await {
                Ok(opened) => handle_sys_session::serialize_open_result(opened),
                Err(err) => handle_sys_session::serialize_error_result(err),
            }
        }
        Err(err) => handle_sys_session::serialize_error_result(err),
    }
}

pub async fn async_close_internal_with_worker_local(
    close_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    let result = handle_sys_session::deserialize_close_param(&close_in).and_then(|session_id| {
        let worker_local = require_worker_local(worker_local)?;
        Ok((session_id, worker_local))
    });
    match result {
        Ok((session_id, worker_local)) => match worker_local.close_async(session_id).await {
            Ok(()) => handle_sys_session::serialize_close_result(),
            Err(err) => handle_sys_session::serialize_error_result(err),
        },
        Err(err) => handle_sys_session::serialize_error_result(err),
    }
}

pub async fn async_put_internal(put_in: Vec<u8>) -> Vec<u8> {
    put_internal(&put_in)
}

pub async fn async_put_internal_with_worker_local(
    put_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    let result = handle_sys_session::deserialize_session_put_param(&put_in).and_then(
        |(session_id, key, value)| {
            let worker_local = require_worker_local(worker_local)?;
            Ok((session_id, key, value, worker_local))
        },
    );
    match result {
        Ok((session_id, key, value, worker_local)) => {
            match worker_local.put_async(session_id, key, value).await {
                Ok(()) => handle_sys_session::serialize_put_result(),
                Err(err) => handle_sys_session::serialize_error_result(err),
            }
        }
        Err(err) => handle_sys_session::serialize_error_result(err),
    }
}

pub async fn async_delete_internal(delete_in: Vec<u8>) -> Vec<u8> {
    delete_internal(&delete_in)
}

pub async fn async_delete_internal_with_worker_local(
    delete_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    let result = handle_sys_session::deserialize_session_delete_param(&delete_in).and_then(
        |(session_id, key)| {
            let worker_local = require_worker_local(worker_local)?;
            Ok((session_id, key, worker_local))
        },
    );
    match result {
        Ok((session_id, key, worker_local)) => {
            match worker_local.delete_async(session_id, &key).await {
                Ok(()) => handle_sys_session::serialize_delete_result(),
                Err(err) => handle_sys_session::serialize_error_result(err),
            }
        }
        Err(err) => handle_sys_session::serialize_error_result(err),
    }
}

pub async fn async_range_internal(range_in: Vec<u8>) -> Vec<u8> {
    range_internal(&range_in)
}

pub async fn async_range_internal_with_worker_local(
    range_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    let result = handle_sys_session::deserialize_session_range_param(&range_in).and_then(
        |(session_id, start, end)| {
            let worker_local = require_worker_local(worker_local)?;
            Ok((session_id, start, end, worker_local))
        },
    );
    match result {
        Ok((session_id, start, end, worker_local)) => {
            match worker_local.range_async(session_id, &start, &end).await {
                Ok(rows) => {
                    let rows = rows
                        .into_iter()
                        .map(|item| (item.key, item.value))
                        .collect::<Vec<_>>();
                    handle_sys_session::serialize_range_result(&rows)
                }
                Err(err) => handle_sys_session::serialize_error_result(err),
            }
        }
        Err(err) => handle_sys_session::serialize_error_result(err),
    }
}

fn require_worker_local(worker_local: Option<WorkerLocalRef>) -> RS<WorkerLocalRef> {
    worker_local.ok_or_else(|| {
        mudu_error!(
            ErrorCode::NotImplemented,
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
        let get = handle_sys_session::serialize_session_get_param(1, b"alpha");
        let err = get_internal_with_worker_local(&get, None).unwrap_err();
        assert!(
            err.to_string()
                .contains("worker local interface is not configured")
        );

        let delete = handle_sys_session::serialize_session_delete_param(1, b"alpha");
        let err = delete_internal_with_worker_local(&delete, None).unwrap_err();
        assert!(
            err.to_string()
                .contains("worker local interface is not configured")
        );
    }

    #[test]
    fn delete_session_codec_round_trips() {
        let encoded = handle_sys_session::serialize_session_delete_param(9, b"alpha");
        let decoded = handle_sys_session::deserialize_session_delete_param(&encoded).unwrap();
        assert_eq!(decoded.0, 9);
        assert_eq!(decoded.1, b"alpha".to_vec());
        handle_sys_session::deserialize_delete_result(
            &handle_sys_session::serialize_delete_result(),
        )
        .unwrap();
    }
}
