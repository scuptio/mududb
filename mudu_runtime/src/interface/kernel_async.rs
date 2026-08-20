#![allow(missing_docs)]

use super::kernel_sync::{
    delete_internal, fs_fstat_internal_with_worker_local, fs_lseek_internal_with_worker_local,
    get_context, get_internal, put_internal, range_internal, relation_conn,
    require_current_session, require_worker_local, wire_to_attr_datums, wire_to_attrs,
    wire_to_deltas,
};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_binding::codec::syscall_payload::{
    decode_close_request, decode_delete_request, decode_fs_close_request, decode_fs_fsync_request,
    decode_fs_open_request, decode_fs_pread_request, decode_fs_pwrite_request,
    decode_fs_read_request, decode_fs_readdir_request, decode_fs_stat_request,
    decode_fs_write_request, decode_get_request, decode_open_request, decode_put_request,
    decode_range_request, decode_relation_get_request, decode_relation_insert_request,
    decode_relation_update_request, encode_close_result, encode_delete_result,
    encode_fs_close_result, encode_fs_fsync_result, encode_fs_open_result, encode_fs_pread_result,
    encode_fs_pwrite_result, encode_fs_read_result, encode_fs_readdir_result,
    encode_fs_stat_result, encode_fs_write_result, encode_get_result, encode_open_result,
    encode_put_result, encode_range_result, encode_relation_get_result,
    encode_relation_insert_result, encode_relation_update_result,
};
use mudu_binding::universal::uni_fs_dirent::UniFsDirent;
use mudu_binding::universal::uni_fs_stat::UniFsStat;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_kernel::server::worker_local::WorkerLocalRef;
use mudu_utils::task_trace;

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

pub async fn async_get_internal(get_in: Vec<u8>) -> Vec<u8> {
    get_internal(&get_in)
}

pub async fn async_open_internal_with_worker_local(
    open_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_open_result(&_async_open_internal(open_in, worker_local).await)
}

async fn _async_open_internal(open_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> RS<OID> {
    let worker_id = decode_open_request(&open_in)?;
    let worker_local = require_worker_local(worker_local)?;
    worker_local.open_argv_async(worker_id.to_oid()).await
}

pub async fn async_close_internal_with_worker_local(
    close_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_close_result(&_async_close_internal(close_in, worker_local).await)
}

async fn _async_close_internal(close_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> RS<()> {
    let session_id = decode_close_request(&close_in)?;
    let worker_local = require_worker_local(worker_local)?;
    worker_local.close_async(session_id.to_oid()).await
}

pub async fn async_get_internal_with_worker_local(
    get_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_get_result(&_async_get_internal(get_in, worker_local).await)
}

async fn _async_get_internal(
    get_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> RS<Option<Vec<u8>>> {
    let (session_id, key) = decode_get_request(&get_in)?;
    let worker_local = require_worker_local(worker_local)?;
    worker_local.get_async(session_id.to_oid(), &key).await
}

pub async fn async_put_internal(put_in: Vec<u8>) -> Vec<u8> {
    put_internal(&put_in)
}

pub async fn async_put_internal_with_worker_local(
    put_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_put_result(&_async_put_internal(put_in, worker_local).await)
}

async fn _async_put_internal(put_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> RS<()> {
    let (session_id, key, value) = decode_put_request(&put_in)?;
    let worker_local = require_worker_local(worker_local)?;
    worker_local
        .put_async(session_id.to_oid(), key, value)
        .await
}

pub async fn async_delete_internal(delete_in: Vec<u8>) -> Vec<u8> {
    delete_internal(&delete_in)
}

pub async fn async_delete_internal_with_worker_local(
    delete_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_delete_result(&_async_delete_internal(delete_in, worker_local).await)
}

async fn _async_delete_internal(
    delete_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> RS<()> {
    let (session_id, key) = decode_delete_request(&delete_in)?;
    let worker_local = require_worker_local(worker_local)?;
    worker_local.delete_async(session_id.to_oid(), &key).await
}

pub async fn async_range_internal(range_in: Vec<u8>) -> Vec<u8> {
    range_internal(&range_in)
}

pub async fn async_range_internal_with_worker_local(
    range_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_range_result(&_async_range_internal(range_in, worker_local).await)
}

async fn _async_range_internal(
    range_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let (session_id, start, end) = decode_range_request(&range_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let rows = worker_local
        .range_async(session_id.to_oid(), &start, &end)
        .await?;
    Ok(rows
        .into_iter()
        .map(|item| (item.key, item.value))
        .collect::<Vec<_>>())
}

/// Point-read one relation row by primary key through the procedure
/// connection (bypasses SQL parsing and result-set serialization).
pub async fn async_relation_get_internal_with_worker_local(
    relation_get_in: Vec<u8>,
    _worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_relation_get_result(&async_relation_get(relation_get_in).await)
}

async fn async_relation_get(relation_get_in: Vec<u8>) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    let (session_id, table, key, select) = decode_relation_get_request(&relation_get_in)?;
    let conn = relation_conn(session_id.to_oid())?;
    conn.relation_get_async(&table, wire_to_attr_datums(key), wire_to_attrs(select))
        .await
}

/// Read-modify-write one relation row by primary key through the procedure
/// connection.
pub async fn async_relation_update_internal_with_worker_local(
    relation_update_in: Vec<u8>,
    _worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_relation_update_result(&async_relation_update(relation_update_in).await)
}

async fn async_relation_update(relation_update_in: Vec<u8>) -> RS<u64> {
    let (session_id, table, key, values, deltas) =
        decode_relation_update_request(&relation_update_in)?;
    let conn = relation_conn(session_id.to_oid())?;
    conn.relation_update_async(
        &table,
        wire_to_attr_datums(key),
        wire_to_attr_datums(values),
        wire_to_deltas(deltas)?,
    )
    .await
}

/// Insert one relation row through the procedure connection.
pub async fn async_relation_insert_internal_with_worker_local(
    relation_insert_in: Vec<u8>,
    _worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_relation_insert_result(&async_relation_insert(relation_insert_in).await)
}

async fn async_relation_insert(relation_insert_in: Vec<u8>) -> RS<()> {
    let (session_id, table, key, values) = decode_relation_insert_request(&relation_insert_in)?;
    let conn = relation_conn(session_id.to_oid())?;
    conn.relation_insert_async(
        &table,
        wire_to_attr_datums(key),
        wire_to_attr_datums(values),
    )
    .await
}

/// Open an fs object through the worker-local fs service.
pub async fn async_fs_open_internal_with_worker_local(
    fs_open_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_open_result(&_async_fs_open_internal(fs_open_in, worker_local).await)
}

async fn _async_fs_open_internal(
    fs_open_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> RS<u32> {
    let argv = decode_fs_open_request(&fs_open_in)?;
    let fs_service = require_worker_local(worker_local)?.fs_service()?;
    fs_service
        .fs_open(
            argv.session.to_oid(),
            argv.oid.to_oid(),
            &argv.path,
            argv.flags,
        )
        .await
}

/// Close an fs fd through the worker-local fs service.
pub async fn async_fs_close_internal_with_worker_local(
    fs_close_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_close_result(&_async_fs_close_internal(fs_close_in, worker_local).await)
}

async fn _async_fs_close_internal(
    fs_close_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> RS<()> {
    let fd = decode_fs_close_request(&fs_close_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    fs_service.fs_close(session_id, fd).await
}

/// Read from an fs fd through the worker-local fs service.
pub async fn async_fs_read_internal_with_worker_local(
    fs_read_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_read_result(&_async_fs_read_internal(fs_read_in, worker_local).await)
}

async fn _async_fs_read_internal(
    fs_read_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> RS<Vec<u8>> {
    let (fd, len) = decode_fs_read_request(&fs_read_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    fs_service.fs_read(session_id, fd, len).await
}

/// Write to an fs fd through the worker-local fs service.
pub async fn async_fs_write_internal_with_worker_local(
    fs_write_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_write_result(&_async_fs_write_internal(fs_write_in, worker_local).await)
}

async fn _async_fs_write_internal(
    fs_write_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> RS<u32> {
    let (fd, data) = decode_fs_write_request(&fs_write_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    fs_service.fs_write(session_id, fd, &data).await
}

/// Read from an fs fd at an offset through the worker-local fs service.
pub async fn async_fs_pread_internal_with_worker_local(
    fs_pread_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_pread_result(&_async_fs_pread_internal(fs_pread_in, worker_local).await)
}

async fn _async_fs_pread_internal(
    fs_pread_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> RS<Vec<u8>> {
    let (fd, offset, len) = decode_fs_pread_request(&fs_pread_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    fs_service.fs_pread(session_id, fd, offset, len).await
}

/// Write to an fs fd at an offset through the worker-local fs service.
pub async fn async_fs_pwrite_internal_with_worker_local(
    fs_pwrite_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_pwrite_result(&_async_fs_pwrite_internal(fs_pwrite_in, worker_local).await)
}

async fn _async_fs_pwrite_internal(
    fs_pwrite_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> RS<()> {
    let (fd, offset, data) = decode_fs_pwrite_request(&fs_pwrite_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    fs_service.fs_pwrite(session_id, fd, offset, &data).await
}

/// Move an fs fd cursor through the worker-local fs service.
pub async fn async_fs_lseek_internal_with_worker_local(
    fs_lseek_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    // fs_lseek is synchronous on the service: reuse the sync handler from kernel_sync.
    fs_lseek_internal_with_worker_local(&fs_lseek_in, worker_local)
}

/// Stat an open fs fd through the worker-local fs service.
pub async fn async_fs_fstat_internal_with_worker_local(
    fs_fstat_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    // fs_fstat is synchronous on the service: reuse the sync handler from kernel_sync.
    fs_fstat_internal_with_worker_local(&fs_fstat_in, worker_local)
}

/// Stat an fs object or entry through the worker-local fs service.
pub async fn async_fs_stat_internal_with_worker_local(
    fs_stat_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_stat_result(&_async_fs_stat_internal(fs_stat_in, worker_local).await)
}

async fn _async_fs_stat_internal(
    fs_stat_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> RS<UniFsStat> {
    let (oid, path) = decode_fs_stat_request(&fs_stat_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    fs_service.fs_stat(session_id, oid.to_oid(), &path).await
}

/// Flush an fs fd through the worker-local fs service.
pub async fn async_fs_fsync_internal_with_worker_local(
    fs_fsync_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_fsync_result(&_async_fs_fsync_internal(fs_fsync_in, worker_local).await)
}

async fn _async_fs_fsync_internal(
    fs_fsync_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> RS<()> {
    let fd = decode_fs_fsync_request(&fs_fsync_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    fs_service.fs_fsync(session_id, fd).await
}

/// List an fs object directory through the worker-local fs service.
pub async fn async_fs_readdir_internal_with_worker_local(
    fs_readdir_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    encode_fs_readdir_result(&_async_fs_readdir_internal(fs_readdir_in, worker_local).await)
}

async fn _async_fs_readdir_internal(
    fs_readdir_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> RS<Vec<UniFsDirent>> {
    let (oid, path) = decode_fs_readdir_request(&fs_readdir_in)?;
    let worker_local = require_worker_local(worker_local)?;
    let fs_service = worker_local.fs_service()?;
    let session_id = require_current_session(&worker_local)?;
    fs_service.fs_readdir(session_id, oid.to_oid(), &path).await
}
