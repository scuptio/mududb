use crate::codec::adapter::{oid_from_mu, oid_to_mu};
use crate::universal::uni_command_argv::UniCommandArgv;
use crate::universal::uni_query_argv::UniQueryArgv;
use crate::universal::uni_sql_param::UniSqlParam;
use crate::universal::uni_sql_stmt::UniSqlStmt;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::common::serde_utils::{deserialize_from, serialize_to_vec};
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::sql_param_value::SQLParamValue;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::database::sql_stmt_text::SQLStmtText;

/// Deserializes a query incoming payload into a statement and parameter pair.
pub fn query_incoming_deserialize(incoming: &[u8]) -> RS<crate::codec::SqlParamPair> {
    let (argument, _) = deserialize_from::<UniQueryArgv>(incoming)?;
    let stmt = argument.query.uni_to()?;
    let params = argument.param_list.uni_to()?;
    let oid = oid_from_mu(argument.oid);
    Ok((oid, Box::new(stmt), Box::new(params)))
}

/// Deserializes a command incoming payload into a statement and parameter pair.
pub fn command_incoming_deserialize(incoming: &[u8]) -> RS<crate::codec::SqlParamPair> {
    let (argument, _) = deserialize_from::<UniCommandArgv>(incoming)?;
    let stmt = argument.command.uni_to()?;
    let params = argument.param_list.uni_to()?;
    let oid = oid_from_mu(argument.oid);
    Ok((oid, Box::new(stmt), Box::new(params)))
}

/// Serializes a statement and its parameters into portable text/value forms.
pub fn incoming_serialize(
    stmt: &dyn SQLStmt,
    param: &dyn SQLParams,
) -> RS<(SQLStmtText, SQLParamValue)> {
    let stmt = SQLStmtText::new(stmt.to_string());
    let desc = param.param_tuple_desc()?;
    if desc.fields().len() as u64 != param.size() {
        return Err(mudu_error!(
            ErrorCode::Decode,
            "tuple size do not as expected"
        ));
    }
    let mut vec = Vec::with_capacity(desc.fields().len());
    for i in 0..param.size() {
        let dat = param.get_idx_unchecked(i);
        let ty = desc.fields()[i as usize].data_type();
        let value = dat.to_value(ty)?;
        vec.push(value)
    }
    Ok((stmt, SQLParamValue::from_vec(vec)))
}

/// Serializes a command request (OID, statement and parameters) into bytes.
pub fn command_incoming_serialize(
    oid: OID,
    stmt: &dyn SQLStmt,
    param: &dyn SQLParams,
) -> RS<Vec<u8>> {
    let (stmt_text, param_value) = incoming_serialize(stmt, param)?;

    let argument = UniCommandArgv {
        oid: oid_to_mu(oid),
        command: UniSqlStmt::uni_from(stmt_text)?,
        param_list: UniSqlParam::uni_from(param_value)?,
    };
    let argument_b = serialize_to_vec(&argument)?;
    Ok(argument_b)
}

/// Serializes a query request (OID, statement and parameters) into bytes.
pub fn query_incoming_serialize(
    oid: OID,
    stmt: &dyn SQLStmt,
    param: &dyn SQLParams,
) -> RS<Vec<u8>> {
    let (stmt_text, param_value) = incoming_serialize(stmt, param)?;

    let argument = UniQueryArgv {
        oid: oid_to_mu(oid),
        query: UniSqlStmt::uni_from(stmt_text)?,
        param_list: UniSqlParam::uni_from(param_value)?,
    };
    let argument_b = serialize_to_vec(&argument)?;
    Ok(argument_b)
}
