#![allow(missing_docs)]

use crate::async_utils::blocking::run_async;
use mudu::common::id::{AttrIndex, OID};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::codec::syscall_payload::{
    decode_close_request, decode_delete_request, decode_fs_close_request, decode_fs_fstat_request,
    decode_fs_fsync_request, decode_fs_lseek_request, decode_fs_open_request,
    decode_fs_pread_request, decode_fs_pwrite_request, decode_fs_read_request,
    decode_fs_readdir_request, decode_fs_stat_request, decode_fs_write_request, decode_get_request,
    decode_open_request, decode_put_request, decode_range_request, decode_relation_get_request,
    decode_relation_insert_request, decode_relation_update_request, encode_close_result,
    encode_delete_result, encode_fs_close_result, encode_fs_fstat_result, encode_fs_fsync_result,
    encode_fs_lseek_result, encode_fs_open_result, encode_fs_pread_result, encode_fs_pwrite_result,
    encode_fs_read_result, encode_fs_readdir_result, encode_fs_stat_result, encode_fs_write_result,
    encode_get_result, encode_open_result, encode_put_result, encode_range_result,
    encode_relation_get_result, encode_relation_insert_result, encode_relation_update_result,
};
use mudu_binding::universal::uni_fs_dirent::UniFsDirent;
use mudu_binding::universal::uni_fs_stat::UniFsStat;
use mudu_binding::universal::uni_relation::{
    RELATION_DELTA_OP_ADD, RELATION_DELTA_OP_ADD_DEFERRED, RELATION_DELTA_OP_SUB,
    RELATION_DELTA_OP_SUB_DEFERRED, RELATION_DELTA_OP_SUB_WRAP_DEFERRED,
};
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::sql::{Context, DBConn};
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_kernel::mudu_conn::mudu_conn_async::MuduConnAsync;
use mudu_kernel::server::worker_local::WorkerLocalRef;
use mudu_kernel::x_engine::DataBin;
use mudu_kernel::x_engine::api::DeltaOp;
use std::any::Any;
use std::sync::Arc;

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

pub(crate) fn get_context(oid: OID) -> RS<Context> {
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
) -> Vec<u8> {
    encode_open_result(&_open_internal(open_in, worker_local))
}

fn _open_internal(open_in: &[u8], worker_local: Option<WorkerLocalRef>) -> RS<OID> {
    let worker_id = decode_open_request(open_in)?;
    let worker_local = require_worker_local(worker_local)?;
    run_async(async move { worker_local.open_argv_async(worker_id.to_oid()).await })?
}

pub fn close_internal_with_worker_local(
    close_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_close_result(&_close_internal(close_in, worker_local))
}

fn _close_internal(close_in: &[u8], worker_local: Option<WorkerLocalRef>) -> RS<()> {
    let session_id = decode_close_request(close_in)?;
    let worker_local = require_worker_local(worker_local)?;
    run_async(async move { worker_local.close_async(session_id.to_oid()).await })?
}

pub fn get_internal(get_in: &[u8]) -> Vec<u8> {
    get_internal_with_worker_local(get_in, None)
}

pub fn get_internal_with_worker_local(
    get_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_get_result(&_get_internal(get_in, worker_local))
}

fn _get_internal(get_in: &[u8], worker_local: Option<WorkerLocalRef>) -> RS<Option<Vec<u8>>> {
    let (session_id, key) = decode_get_request(get_in)?;
    let worker_local = require_worker_local(worker_local)?;
    run_async(async move { worker_local.get_async(session_id.to_oid(), &key).await })?
}

pub fn put_internal(put_in: &[u8]) -> Vec<u8> {
    put_internal_with_worker_local(put_in, None)
}

pub fn put_internal_with_worker_local(
    put_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_put_result(&_put_internal(put_in, worker_local))
}

fn _put_internal(put_in: &[u8], worker_local: Option<WorkerLocalRef>) -> RS<()> {
    let (session_id, key, value) = decode_put_request(put_in)?;
    let worker_local = require_worker_local(worker_local)?;
    run_async(async move {
        worker_local
            .put_async(session_id.to_oid(), key, value)
            .await
    })?
}

pub fn delete_internal(delete_in: &[u8]) -> Vec<u8> {
    delete_internal_with_worker_local(delete_in, None)
}

pub fn delete_internal_with_worker_local(
    delete_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_delete_result(&_delete_internal(delete_in, worker_local))
}

fn _delete_internal(delete_in: &[u8], worker_local: Option<WorkerLocalRef>) -> RS<()> {
    let (session_id, key) = decode_delete_request(delete_in)?;
    let worker_local = require_worker_local(worker_local)?;
    run_async(async move { worker_local.delete_async(session_id.to_oid(), &key).await })?
}

pub fn range_internal(range_in: &[u8]) -> Vec<u8> {
    range_internal_with_worker_local(range_in, None)
}

pub fn range_internal_with_worker_local(
    range_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_range_result(&_range_internal(range_in, worker_local))
}

fn _range_internal(
    range_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let (session_id, start, end) = decode_range_request(range_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let result = run_async(async move {
        worker_local
            .range_async(session_id.to_oid(), &start, &end)
            .await
    })??;
    Ok(result
        .into_iter()
        .map(|item| (item.key, item.value))
        .collect::<Vec<_>>())
}

/// Point-read one relation row by primary key through the procedure
/// connection (bypasses SQL parsing and result-set serialization).
pub fn relation_get_internal_with_worker_local(
    relation_get_in: &[u8],
    _worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_relation_get_result(&_relation_get_internal(relation_get_in))
}

pub(crate) fn _relation_get_internal(relation_get_in: &[u8]) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    let (session_id, table, key, select) = decode_relation_get_request(relation_get_in)?;
    let conn = relation_conn(session_id.to_oid())?;
    run_async(async move {
        conn.relation_get_async(&table, wire_to_attr_datums(key), wire_to_attrs(select))
            .await
    })?
}

/// Read-modify-write one relation row by primary key through the procedure
/// connection.
pub fn relation_update_internal_with_worker_local(
    relation_update_in: &[u8],
    _worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_relation_update_result(&_relation_update_internal(relation_update_in))
}

pub(crate) fn _relation_update_internal(relation_update_in: &[u8]) -> RS<u64> {
    let (session_id, table, key, values, deltas) =
        decode_relation_update_request(relation_update_in)?;
    let conn = relation_conn(session_id.to_oid())?;
    run_async(async move {
        conn.relation_update_async(
            &table,
            wire_to_attr_datums(key),
            wire_to_attr_datums(values),
            wire_to_deltas(deltas)?,
        )
        .await
    })?
}

/// Insert one relation row through the procedure connection.
pub fn relation_insert_internal_with_worker_local(
    relation_insert_in: &[u8],
    _worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_relation_insert_result(&_relation_insert_internal(relation_insert_in))
}

pub(crate) fn _relation_insert_internal(relation_insert_in: &[u8]) -> RS<()> {
    let (session_id, table, key, values) = decode_relation_insert_request(relation_insert_in)?;
    let conn = relation_conn(session_id.to_oid())?;
    run_async(async move {
        conn.relation_insert_async(
            &table,
            wire_to_attr_datums(key),
            wire_to_attr_datums(values),
        )
        .await
    })?
}

/// Resolve the relation connection behind the session id carried by a
/// relation frame.
///
/// The id in the frame is the task id the procedure invocation was bound to
/// (`Context::create` in `app_inst_impl`); resolve it through the task
/// `Context` — the same path the SQL syscalls use — and downcast its async
/// connection to `MuduConnAsync`, which owns the worker-local session the
/// current transaction was begun on. The frame's session field is therefore
/// only a lookup key: the session the relation operation runs under comes
/// from the connection, not from the guest.
pub(crate) fn relation_conn(oid: OID) -> RS<Arc<MuduConnAsync>> {
    let context = get_context(oid)?;
    let DBConn::Async(conn) = context.db_conn() else {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            "relation syscalls require an async procedure connection"
        ));
    };
    let any: Arc<dyn Any + Send + Sync> = conn.clone();
    any.downcast::<MuduConnAsync>().map_err(|_| {
        mudu_error!(
            ErrorCode::NotImplemented,
            "relation syscalls require a mudu worker-local connection"
        )
    })
}

/// Convert wire `(attr, datum)` pairs into kernel `(AttrIndex, DataBin)`
/// pairs.
pub(crate) fn wire_to_attr_datums(items: Vec<(u64, Vec<u8>)>) -> Vec<(AttrIndex, DataBin)> {
    items
        .into_iter()
        .map(|(attr, datum)| (attr as AttrIndex, datum))
        .collect()
}

/// Convert wire attribute indices into kernel `AttrIndex` values.
pub(crate) fn wire_to_attrs(items: Vec<u64>) -> Vec<AttrIndex> {
    items.into_iter().map(|attr| attr as AttrIndex).collect()
}

/// Convert wire `(attr, op, datum)` triples into kernel delta assignments,
/// rejecting unknown op codes.
pub(crate) fn wire_to_deltas(
    items: Vec<(u64, u8, Vec<u8>)>,
) -> RS<Vec<(AttrIndex, DeltaOp, DataBin)>> {
    items
        .into_iter()
        .map(|(attr, op, datum)| {
            let op = match op {
                RELATION_DELTA_OP_ADD => DeltaOp::Add,
                RELATION_DELTA_OP_SUB => DeltaOp::Sub,
                RELATION_DELTA_OP_ADD_DEFERRED => DeltaOp::AddDeferred,
                RELATION_DELTA_OP_SUB_DEFERRED => DeltaOp::SubDeferred,
                RELATION_DELTA_OP_SUB_WRAP_DEFERRED => DeltaOp::SubWrapDeferred,
                other => {
                    return Err(mudu_error!(
                        ErrorCode::Decode,
                        format!("unknown relation delta op {other}")
                    ));
                }
            };
            Ok((attr as AttrIndex, op, datum))
        })
        .collect()
}

/// Open an fs object through the worker-local fs service.
pub fn fs_open_internal_with_worker_local(
    fs_open_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_open_result(&_fs_open_internal(fs_open_in, worker_local))
}

fn _fs_open_internal(fs_open_in: &[u8], worker_local: Option<WorkerLocalRef>) -> RS<u32> {
    let argv = decode_fs_open_request(fs_open_in)?;
    let fs_service = require_worker_local(worker_local)?.fs_service()?;
    run_async(async move {
        fs_service
            .fs_open(
                argv.session.to_oid(),
                argv.oid.to_oid(),
                &argv.path,
                argv.flags,
            )
            .await
    })?
}

/// Close an fs fd through the worker-local fs service.
pub fn fs_close_internal_with_worker_local(
    fs_close_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_close_result(&_fs_close_internal(fs_close_in, worker_local))
}

fn _fs_close_internal(fs_close_in: &[u8], worker_local: Option<WorkerLocalRef>) -> RS<()> {
    let fd = decode_fs_close_request(fs_close_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    run_async(async move { fs_service.fs_close(session_id, fd).await })?
}

/// Read from an fs fd through the worker-local fs service.
pub fn fs_read_internal_with_worker_local(
    fs_read_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_read_result(&_fs_read_internal(fs_read_in, worker_local))
}

fn _fs_read_internal(fs_read_in: &[u8], worker_local: Option<WorkerLocalRef>) -> RS<Vec<u8>> {
    let (fd, len) = decode_fs_read_request(fs_read_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    run_async(async move { fs_service.fs_read(session_id, fd, len).await })?
}

/// Write to an fs fd through the worker-local fs service.
pub fn fs_write_internal_with_worker_local(
    fs_write_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_write_result(&_fs_write_internal(fs_write_in, worker_local))
}

fn _fs_write_internal(fs_write_in: &[u8], worker_local: Option<WorkerLocalRef>) -> RS<u32> {
    let (fd, data) = decode_fs_write_request(fs_write_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    run_async(async move { fs_service.fs_write(session_id, fd, &data).await })?
}

/// Read from an fs fd at an offset through the worker-local fs service.
pub fn fs_pread_internal_with_worker_local(
    fs_pread_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_pread_result(&_fs_pread_internal(fs_pread_in, worker_local))
}

fn _fs_pread_internal(fs_pread_in: &[u8], worker_local: Option<WorkerLocalRef>) -> RS<Vec<u8>> {
    let (fd, offset, len) = decode_fs_pread_request(fs_pread_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    run_async(async move { fs_service.fs_pread(session_id, fd, offset, len).await })?
}

/// Write to an fs fd at an offset through the worker-local fs service.
pub fn fs_pwrite_internal_with_worker_local(
    fs_pwrite_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_pwrite_result(&_fs_pwrite_internal(fs_pwrite_in, worker_local))
}

fn _fs_pwrite_internal(fs_pwrite_in: &[u8], worker_local: Option<WorkerLocalRef>) -> RS<()> {
    let (fd, offset, data) = decode_fs_pwrite_request(fs_pwrite_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    run_async(async move { fs_service.fs_pwrite(session_id, fd, offset, &data).await })?
}

/// Move an fs fd cursor through the worker-local fs service.
pub fn fs_lseek_internal_with_worker_local(
    fs_lseek_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_lseek_result(&_fs_lseek_internal(fs_lseek_in, worker_local))
}

fn _fs_lseek_internal(fs_lseek_in: &[u8], worker_local: Option<WorkerLocalRef>) -> RS<u64> {
    let (fd, offset, whence) = decode_fs_lseek_request(fs_lseek_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    // fs_lseek is a pure in-memory operation on the service: no async hop.
    fs_service.fs_lseek(session_id, fd, offset, whence)
}

/// Stat an open fs fd through the worker-local fs service.
pub fn fs_fstat_internal_with_worker_local(
    fs_fstat_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_fstat_result(&_fs_fstat_internal(fs_fstat_in, worker_local))
}

fn _fs_fstat_internal(fs_fstat_in: &[u8], worker_local: Option<WorkerLocalRef>) -> RS<UniFsStat> {
    let fd = decode_fs_fstat_request(fs_fstat_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    // fs_fstat is a pure in-memory operation on the service: no async hop.
    fs_service.fs_fstat(session_id, fd)
}

/// Stat an fs object or entry through the worker-local fs service.
pub fn fs_stat_internal_with_worker_local(
    fs_stat_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_stat_result(&_fs_stat_internal(fs_stat_in, worker_local))
}

fn _fs_stat_internal(fs_stat_in: &[u8], worker_local: Option<WorkerLocalRef>) -> RS<UniFsStat> {
    let (oid, path) = decode_fs_stat_request(fs_stat_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    run_async(async move { fs_service.fs_stat(session_id, oid.to_oid(), &path).await })?
}

/// Flush an fs fd through the worker-local fs service.
pub fn fs_fsync_internal_with_worker_local(
    fs_fsync_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_fsync_result(&_fs_fsync_internal(fs_fsync_in, worker_local))
}

fn _fs_fsync_internal(fs_fsync_in: &[u8], worker_local: Option<WorkerLocalRef>) -> RS<()> {
    let fd = decode_fs_fsync_request(fs_fsync_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    run_async(async move { fs_service.fs_fsync(session_id, fd).await })?
}

/// List an fs object directory through the worker-local fs service.
pub fn fs_readdir_internal_with_worker_local(
    fs_readdir_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_readdir_result(&_fs_readdir_internal(fs_readdir_in, worker_local))
}

fn _fs_readdir_internal(
    fs_readdir_in: &[u8],
    worker_local: Option<WorkerLocalRef>,
) -> RS<Vec<UniFsDirent>> {
    let (oid, path) = decode_fs_readdir_request(fs_readdir_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    run_async(async move { fs_service.fs_readdir(session_id, oid.to_oid(), &path).await })?
}

pub(crate) fn require_worker_local(worker_local: Option<WorkerLocalRef>) -> RS<WorkerLocalRef> {
    worker_local.ok_or_else(|| {
        mudu_error!(
            ErrorCode::NotImplemented,
            "worker local interface is not configured for this runtime path"
        )
    })
}

/// Resolve the session the calling procedure is bound to.
///
/// The SyscallPayload v1 fs frames carry no session id outside `fs-open`:
/// fd-based operations and `fs-stat`/`fs-readdir` operate on the session the
/// guest procedure invocation is bound to.
pub(crate) fn require_current_session(worker_local: &WorkerLocalRef) -> RS<OID> {
    worker_local.current_session_id().ok_or_else(|| {
        mudu_error!(
            ErrorCode::NotImplemented,
            "fs syscalls require a session-bound worker local interface"
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
    use mudu_binding::codec::syscall_payload::{
        decode_delete_result, decode_get_result, encode_delete_request, encode_get_request,
    };
    use mudu_binding::universal::uni_oid::UniOid;

    #[test]
    fn kv_syscalls_require_worker_local() {
        let get = encode_get_request(UniOid::from_oid(1), b"alpha");
        let err = decode_get_result(&get_internal_with_worker_local(&get, None)).unwrap_err();
        assert!(
            err.message()
                .contains("worker local interface is not configured")
        );

        let delete = encode_delete_request(UniOid::from_oid(1), b"alpha");
        let err =
            decode_delete_result(&delete_internal_with_worker_local(&delete, None)).unwrap_err();
        assert!(
            err.message()
                .contains("worker local interface is not configured")
        );
    }

    #[test]
    fn kv_error_frames_carry_the_matching_message_kind() {
        let delete = encode_delete_request(UniOid::from_oid(9), b"alpha");
        let out = delete_internal_with_worker_local(&delete, None);
        let err = decode_delete_result(&out).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    }
}
