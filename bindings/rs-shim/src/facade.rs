use crate::error;
use crate::exports::mududb::component_shim::types;
use crate::ids;
use crate::result::ResultSet;
use crate::statement::SqlStmt;
use crate::value_list::ValueList;
use mududb::binding::universal::uni_session_open_argv::UniSessionOpenArgv;

type ShimResult<T> = Result<T, types::Error>;

pub fn open(uri: &str) -> ShimResult<types::Oid> {
    let oid = if uri.is_empty() {
        mududb::sys_interface::api::mudu_open()
    } else {
        let worker_id = uri.parse::<u128>().map_err(|_| {
            error::unsupported("open uri must be empty or a numeric worker object id")
        })?;
        mududb::sys_interface::api::mudu_open_argv(&UniSessionOpenArgv::new(worker_id))
    }
    .map_err(error::from_mudu)?;
    Ok(ids::from_facade(oid))
}

pub fn close(id: types::Oid) -> ShimResult<()> {
    mududb::sys_interface::api::mudu_close(ids::to_facade(id)).map_err(error::from_mudu)
}

pub fn query(id: types::Oid, stmt: &SqlStmt, values: &ValueList) -> ShimResult<ResultSet> {
    let facade_values = values.to_facade_values()?;
    let payload = mududb::binding::system::query_invoke::serialize_query_dyn_param(
        ids::to_facade(id),
        stmt.as_string(),
        &facade_values,
    )
    .map_err(error::from_mudu)?;
    let result =
        mududb::sys_interface::api::mudu_query_bytes(&payload).map_err(error::from_mudu)?;
    let (batch, desc) = mududb::binding::system::query_invoke::deserialize_query_result(&result)
        .map_err(error::from_mudu)?;
    Ok(ResultSet::from_facade(batch, desc))
}

pub fn command(id: types::Oid, stmt: &SqlStmt, values: &ValueList) -> ShimResult<u64> {
    invoke_command(
        id,
        stmt,
        values,
        mududb::sys_interface::api::mudu_command_bytes,
    )
}

pub fn batch(id: types::Oid, stmt: &SqlStmt, values: &ValueList) -> ShimResult<u64> {
    invoke_command(
        id,
        stmt,
        values,
        mududb::sys_interface::api::mudu_batch_bytes,
    )
}

fn invoke_command(
    id: types::Oid,
    stmt: &SqlStmt,
    values: &ValueList,
    invoke: fn(&[u8]) -> mududb::mudu::common::result::RS<Vec<u8>>,
) -> ShimResult<u64> {
    let facade_values = values.to_facade_values()?;
    let payload = mududb::binding::system::command_invoke::serialize_command_param(
        ids::to_facade(id),
        stmt.as_string(),
        &facade_values,
    )
    .map_err(error::from_mudu)?;
    let result = invoke(&payload).map_err(error::from_mudu)?;
    mududb::binding::system::command_invoke::deserialize_command_result(&result)
        .map_err(error::from_mudu)
}

pub fn fs_open(session: types::Oid, oid: types::Oid, path: &str, flags: u32) -> ShimResult<u32> {
    mududb::sys_interface::api::mudu_fs_open(
        ids::to_facade(session),
        ids::to_facade(oid),
        path,
        flags,
    )
    .map_err(error::from_mudu)
}

pub fn fs_close(session: types::Oid, fd: u32) -> ShimResult<()> {
    mududb::sys_interface::api::mudu_fs_close(ids::to_facade(session), fd).map_err(error::from_mudu)
}

pub fn fs_read(session: types::Oid, fd: u32, len: u32) -> ShimResult<Vec<u8>> {
    mududb::sys_interface::api::mudu_fs_read(ids::to_facade(session), fd, len)
        .map_err(error::from_mudu)
}

pub fn fs_write(session: types::Oid, fd: u32, data: &[u8]) -> ShimResult<u32> {
    mududb::sys_interface::api::mudu_fs_write(ids::to_facade(session), fd, data)
        .map_err(error::from_mudu)
}

pub fn fs_pread(session: types::Oid, fd: u32, offset: u64, len: u32) -> ShimResult<Vec<u8>> {
    mududb::sys_interface::api::mudu_fs_pread(ids::to_facade(session), fd, offset, len)
        .map_err(error::from_mudu)
}

pub fn fs_pwrite(session: types::Oid, fd: u32, offset: u64, data: &[u8]) -> ShimResult<()> {
    mududb::sys_interface::api::mudu_fs_pwrite(ids::to_facade(session), fd, offset, data)
        .map_err(error::from_mudu)
}

pub fn fs_lseek(session: types::Oid, fd: u32, offset: i64, whence: u32) -> ShimResult<u64> {
    mududb::sys_interface::api::mudu_fs_lseek(ids::to_facade(session), fd, offset, whence)
        .map_err(error::from_mudu)
}

pub fn fs_fstat(session: types::Oid, fd: u32) -> ShimResult<types::FileStat> {
    mududb::sys_interface::api::mudu_fs_fstat(ids::to_facade(session), fd)
        .map(fs_stat_from_facade)
        .map_err(error::from_mudu)
}

pub fn fs_stat(session: types::Oid, oid: types::Oid, path: &str) -> ShimResult<types::FileStat> {
    mududb::sys_interface::api::mudu_fs_stat(ids::to_facade(session), ids::to_facade(oid), path)
        .map(fs_stat_from_facade)
        .map_err(error::from_mudu)
}

pub fn fs_fsync(session: types::Oid, fd: u32) -> ShimResult<()> {
    mududb::sys_interface::api::mudu_fs_fsync(ids::to_facade(session), fd).map_err(error::from_mudu)
}

pub fn fs_readdir(
    session: types::Oid,
    oid: types::Oid,
    path: &str,
) -> ShimResult<Vec<types::FsDirent>> {
    mududb::sys_interface::api::mudu_fs_readdir(ids::to_facade(session), ids::to_facade(oid), path)
        .map(|entries| entries.into_iter().map(fs_dirent_from_facade).collect())
        .map_err(error::from_mudu)
}

fn fs_stat_from_facade(stat: mududb::sys_interface::api::FsStat) -> types::FileStat {
    types::FileStat {
        oid: ids::from_facade(stat.oid),
        generation: stat.generation,
        entry: stat.entry,
        length: stat.length,
        state: stat.state,
    }
}

fn fs_dirent_from_facade(entry: mududb::sys_interface::api::FsDirEntry) -> types::FsDirent {
    types::FsDirent {
        name: entry.name,
        is_dir: entry.is_dir,
        length: entry.length,
    }
}
