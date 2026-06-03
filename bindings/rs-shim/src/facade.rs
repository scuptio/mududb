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
