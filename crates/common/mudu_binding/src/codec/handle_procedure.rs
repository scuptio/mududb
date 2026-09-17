use crate::codec::adapter::{error_from_mu, error_to_mu};
use crate::universal::mp_wire::{FromValue, ToValue, Value, decode_value_prefix, encode_value};
use crate::universal::uni_error::UniError;
use crate::universal::uni_procedure_param::UniProcedureParam;
use crate::universal::uni_procedure_result::UniProcedureResult;
use crate::universal::uni_result::UniResult;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::json::{JsonValue, to_json_value};
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;

/// Maps a wire-runtime error to a `MuduError` with `ErrorCode::Decode`.
fn wire_err(e: crate::universal::mp_wire::WireError) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::Decode, e.message)
}

/// Decodes one wire value from the start of `bytes`; trailing bytes are
/// tolerated because the FFI channel passes fixed-size output buffers.
fn decode_prefix(bytes: &[u8]) -> RS<Value> {
    let (value, _) = decode_value_prefix(bytes).map_err(wire_err)?;
    Ok(value)
}

/// Deserializes a procedure parameter from its universal representation.
pub fn procedure_deserialize_param(param: &[u8]) -> RS<ProcedureParam> {
    let param = UniProcedureParam::from_value(&decode_prefix(param)?).map_err(wire_err)?;
    let proc_param = param.uni_to()?;
    Ok(proc_param)
}

/// Serializes a procedure parameter into its universal representation.
pub fn procedure_serialize_param(param: ProcedureParam) -> Vec<u8> {
    let r = _procedure_serialize_param(param);
    r.unwrap_or_default()
}

fn _procedure_serialize_param(param: ProcedureParam) -> RS<Vec<u8>> {
    let mu_proc_param = UniProcedureParam::uni_from(param)?;
    Ok(encode_value(&mu_proc_param.to_value()))
}

/// Serializes a procedure result (or error) into its universal representation.
pub fn procedure_serialize_result(result: RS<ProcedureResult>) -> Vec<u8> {
    let r = _procedure_serialize_result(result);
    r.unwrap_or_default()
}

/// Deserializes a procedure result from its universal representation.
pub fn procedure_deserialize_result(result: &[u8]) -> RS<ProcedureResult> {
    _procedure_deserialize_result(result)
}

fn _procedure_deserialize_result(result: &[u8]) -> RS<ProcedureResult> {
    let mu_result: UniResult<UniProcedureResult, UniError> =
        UniResult::from_value(&decode_prefix(result)?).map_err(wire_err)?;
    match mu_result {
        UniResult::Ok(mu_procedure_result) => {
            let mu_p_r = mu_procedure_result.uni_to()?;
            Ok(mu_p_r)
        }
        UniResult::Err(mu_error) => Err(error_from_mu(mu_error)),
    }
}

fn _procedure_serialize_result(result: RS<ProcedureResult>) -> RS<Vec<u8>> {
    let mu_result: UniResult<UniProcedureResult, UniError> = match result {
        Ok(proc_result) => {
            let result = UniProcedureResult::uni_from(proc_result);
            match result {
                Ok(mu_proc_result) => UniResult::Ok(mu_proc_result),
                Err(e) => UniResult::Err(error_to_mu(e)),
            }
        }
        Err(error) => UniResult::Err(error_to_mu(error.clone())),
    };
    Ok(encode_value(&mu_result.to_value()))
}

/// Converts a procedure result into a JSON value.
pub fn result_to_json(r: ProcedureResult) -> RS<JsonValue> {
    let result_mu = UniProcedureResult::uni_from(r)?;
    to_json_value(&result_mu)
}

#[cfg(test)]
mod test {
    use crate::system::command_invoke::{deserialize_command_result, serialize_command_result};
    use mudu::common::result::RS;
    use mudu::error::ErrorCode;
    use mudu::mudu_error;

    #[test]
    fn test_mu_result() {
        let result: RS<u64> = Err(mudu_error!(ErrorCode::Database, "db error"));
        let s = serialize_command_result(result);
        let de_result = deserialize_command_result(&s);
        assert!(
            de_result.is_err()
                && de_result.as_ref().expect_err("expected error").ec() == ErrorCode::Database
        );
        println!("{:?}", de_result)
    }
}
