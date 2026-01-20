use crate::codec::adapter::{error_from_mu, error_to_mu};
use crate::universal::uni_error::UniError;
use crate::universal::uni_procedure_param::UniProcedureParam;
use crate::universal::uni_procedure_result::UniProcedureResult;
use crate::universal::uni_result::UniResult;
use mudu::common::result::RS;
use mudu::common::serde_utils::{deserialize_from, serialize_to_vec};
use mudu::utils::json::{JsonValue, to_json_value};
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;

pub fn procedure_deserialize_param(param: &[u8]) -> RS<ProcedureParam> {
    let (param, _) = deserialize_from::<UniProcedureParam>(param)?;
    let proc_param = param.uni_to()?;
    Ok(proc_param)
}

pub fn procedure_serialize_param(param: ProcedureParam) -> Vec<u8> {
    let r = _procedure_serialize_param(param);
    r.unwrap_or_default()
}

fn _procedure_serialize_param(param: ProcedureParam) -> RS<Vec<u8>> {
    let mu_proc_param = UniProcedureParam::uni_from(param)?;
    serialize_to_vec(&mu_proc_param)
}

pub fn procedure_serialize_result(result: RS<ProcedureResult>) -> Vec<u8> {
    let r = _procedure_serialize_result(result);
    r.unwrap_or_default()
}

pub fn procedure_deserialize_result(result: &[u8]) -> RS<ProcedureResult> {
    _procedure_deserialize_result(result)
}

fn _procedure_deserialize_result(result: &[u8]) -> RS<ProcedureResult> {
    let (mu_result, _) = deserialize_from::<UniResult<UniProcedureResult, UniError>>(result)?;
    match mu_result {
        UniResult::Ok(mu_procedure_result) => {
            let mu_p_r = mu_procedure_result.uni_to()?;
            Ok(mu_p_r)
        }
        UniResult::Err(mu_error) => Err(error_from_mu(mu_error)),
    }
}

fn _procedure_serialize_result(result: RS<ProcedureResult>) -> RS<Vec<u8>> {
    let mu_result = match result {
        Ok(proc_result) => {
            let result = UniProcedureResult::uni_from(proc_result);
            match result {
                Ok(mu_proc_result) => UniResult::Ok(mu_proc_result),
                Err(e) => UniResult::Err(error_to_mu(e)),
            }
        }
        Err(error) => UniResult::Err(error_to_mu(error.clone())),
    };
    serialize_to_vec(&mu_result)
}

pub fn result_to_json(r: ProcedureResult) -> RS<JsonValue> {
    let result_mu = UniProcedureResult::uni_from(r)?;
    to_json_value(&result_mu)
}

#[cfg(test)]
mod test {
    use crate::system::command_invoke::{deserialize_command_result, serialize_command_result};
    use mudu::common::result::RS;
    use mudu::error::ec::EC;
    use mudu::m_error;

    #[test]
    fn test_mu_result() {
        let result: RS<u64> = Err(m_error!(EC::DBInternalError, "db error"));
        let s = serialize_command_result(result);
        let de_result = deserialize_command_result(&s);
        assert!(
            de_result.is_err()
                && de_result.as_ref().expect_err("expected error").ec() == EC::DBInternalError
        );
        println!("{:?}", de_result)
    }
}
