use crate::codec::handle_procedure;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::error::err::MError;
use mudu::m_error;
use mudu::utils::json::JsonValue;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use std::slice;
use tracing::{error, info};

fn _invoke_proc(
    param: Vec<u8>,
    f: fn(ProcedureParam) -> RS<ProcedureResult>,
) -> RS<ProcedureResult> {
    let r = deserialize_param(&param)?;
    let result = f(r)?;
    Ok(result)
}

async fn _invoke_proc_async<F>(param: Vec<u8>, f: F) -> RS<ProcedureResult>
where
    F: AsyncFn(&ProcedureParam) -> RS<ProcedureResult>,
{
    let r = deserialize_param(&param)?;
    let result = f(&r).await?;
    Ok(result)
}

pub fn serialize_param(p: ProcedureParam) -> RS<Vec<u8>> {
    let r = handle_procedure::procedure_serialize_param(p);
    Ok(r)
}

pub fn deserialize_param(p: &[u8]) -> RS<ProcedureParam> {
    if p.is_empty() {
        return Err(m_error!(EC::DecodeErr, "cannot deserialize param"));
    }
    handle_procedure::procedure_deserialize_param(p)
}

pub fn serialize_result(p: RS<ProcedureResult>) -> RS<Vec<u8>> {
    let r = handle_procedure::procedure_serialize_result(p);
    Ok(r)
}

pub fn deserialize_result(r: &[u8]) -> RS<ProcedureResult> {
    if r.is_empty() {
        return Err(m_error!(EC::DecodeErr, "cannot deserialize result"));
    }
    handle_procedure::procedure_deserialize_result(r)
}

pub fn invoke_procedure(param: Vec<u8>, f: fn(ProcedureParam) -> RS<ProcedureResult>) -> Vec<u8> {
    let r = _invoke_proc(param, f);
    handle_procedure::procedure_serialize_result(r)
}

pub async fn invoke_procedure_async<F>(param: Vec<u8>, f: F) -> Vec<u8>
where
    F: AsyncFn(&ProcedureParam) -> RS<ProcedureResult>,
{
    let r = _invoke_proc_async(param, f).await;
    handle_procedure::procedure_serialize_result(r)
}

pub fn result_to_json(r: ProcedureResult) -> RS<JsonValue> {
    handle_procedure::result_to_json(r)
}

pub fn invoke_wrapper(
    p1_ptr: *const u8,
    p1_len: usize,
    p2_ptr: *mut u8,
    p2_len: usize,
    proc: fn(&ProcedureParam) -> RS<ProcedureResult>,
) -> i32 {
    let r = _invoke_wrapper(p1_ptr, p1_len, p2_ptr, p2_len, proc);
    match r {
        Ok(()) => 0,
        Err((code, _e)) => code,
    }
}

fn _invoke_wrapper(
    p1_ptr: *const u8,
    p1_len: usize,
    p2_ptr: *mut u8,
    p2_len: usize,
    f: fn(&ProcedureParam) -> RS<ProcedureResult>,
) -> Result<(), (i32, MError)> {
    let param: ProcedureParam = unsafe {
        let slice = slice::from_raw_parts(p1_ptr, p1_len);
        let param = deserialize_param(slice).map_err(|e| {
            error!(
                "deserialized input parameter error {}, length {}",
                e, p1_len
            );
            (-1001, e)
        })?;
        param
    };
    info!("invoke function, param {:?}", &param);
    let result = f(&param);
    info!("invoke function, return {:?}", &result);
    let out_buf = unsafe {
        let slice = slice::from_raw_parts_mut(p2_ptr, p2_len);
        slice
    };

    let result_b = serialize_result(result).map_err(|e| (-2002, e))?;
    if result_b.len() > out_buf.len() {
        return Err((
            -2024,
            m_error!(EC::InsufficientBufferSpace, "insufficient buffer space"),
        ));
    }
    out_buf.copy_from_slice(&result_b);
    Ok(())
}
