use crate::codec::adapter::{error_from_mu, error_to_mu};
use crate::codec::handle_sys_incoming;
use crate::universal::uni_error::UniError;
use crate::universal::uni_result::UniResult;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::common::serde_utils::{deserialize_from, serialize_to_vec};
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

pub fn command_param_serialize(oid: OID, stmt: &dyn SQLStmt, param: &dyn SQLParams) -> RS<Vec<u8>> {
    handle_sys_incoming::command_incoming_serialize(oid, stmt, param)
}

pub fn command_param_deserialize(param: &[u8]) -> RS<(OID, Box<dyn SQLStmt>, Box<dyn SQLParams>)> {
    handle_sys_incoming::command_incoming_deserialize(param)
}

pub fn command_result_serialize(result: RS<u64>) -> Vec<u8> {
    let mu_r = UniResult::from(result).map_err(error_to_mu);
    let mu_r_bin = serialize_to_vec(&mu_r).unwrap_or_default();
    mu_r_bin
}

pub fn command_result_deserialize(result: &[u8]) -> RS<u64> {
    let (mu_result, _) = deserialize_from::<UniResult<u64, UniError>>(result)?;
    let affected_rows = <UniResult<u64, UniError> as Into<Result<u64, UniError>>>::into(mu_result)
        .map_err(error_from_mu)?;
    Ok(affected_rows)
}
