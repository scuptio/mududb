use crate::codec::handle_procedure;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use mudu::utils::json::JsonValue;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use std::future::Future;
use std::slice;
use tracing::{debug, error};

fn _invoke_proc(
    param: Vec<u8>,
    f: fn(ProcedureParam) -> RS<ProcedureResult>,
) -> RS<ProcedureResult> {
    let r = deserialize_param(&param)?;
    let result = f(r)?;
    Ok(result)
}

async fn _invoke_proc_async<F, Fut>(param: Vec<u8>, f: F) -> RS<ProcedureResult>
where
    F: FnOnce(ProcedureParam) -> Fut,
    Fut: Future<Output = RS<ProcedureResult>>,
{
    let r = deserialize_param(&param)?;
    let result = f(r).await?;
    Ok(result)
}

/// Serializes a procedure parameter into bytes.
pub fn serialize_param(p: ProcedureParam) -> RS<Vec<u8>> {
    let r = handle_procedure::procedure_serialize_param(p);
    Ok(r)
}

/// Deserializes a procedure parameter from bytes.
pub fn deserialize_param(p: &[u8]) -> RS<ProcedureParam> {
    if p.is_empty() {
        return Err(mudu_error!(ErrorCode::Decode, "cannot deserialize param"));
    }
    handle_procedure::procedure_deserialize_param(p)
}

/// Serializes a procedure result (or error) into bytes.
pub fn serialize_result(p: RS<ProcedureResult>) -> RS<Vec<u8>> {
    let r = handle_procedure::procedure_serialize_result(p);
    Ok(r)
}

/// Deserializes a procedure result from bytes.
pub fn deserialize_result(r: &[u8]) -> RS<ProcedureResult> {
    if r.is_empty() {
        return Err(mudu_error!(ErrorCode::Decode, "cannot deserialize result"));
    }
    handle_procedure::procedure_deserialize_result(r)
}

/// Invokes a synchronous procedure after deserializing its parameter bytes.
pub fn invoke_procedure(param: Vec<u8>, f: fn(ProcedureParam) -> RS<ProcedureResult>) -> Vec<u8> {
    let r = _invoke_proc(param, f);
    handle_procedure::procedure_serialize_result(r)
}

/// Invokes an asynchronous procedure after deserializing its parameter bytes.
pub async fn invoke_procedure_async<F, Fut>(param: Vec<u8>, f: F) -> Vec<u8>
where
    F: FnOnce(ProcedureParam) -> Fut,
    Fut: Future<Output = RS<ProcedureResult>>,
{
    let r = _invoke_proc_async(param, f).await;
    handle_procedure::procedure_serialize_result(r)
}

/// Converts a procedure result into a JSON value.
pub fn result_to_json(r: ProcedureResult) -> RS<JsonValue> {
    handle_procedure::result_to_json(r)
}

/// FFI-compatible wrapper that deserializes input, calls `proc` and writes output.
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
) -> Result<(), (i32, MuduError)> {
    let param: ProcedureParam = unsafe {
        let slice = slice::from_raw_parts(p1_ptr, p1_len);

        deserialize_param(slice).map_err(|e| {
            error!(
                "deserialized input parameter error {}, length {}",
                e, p1_len
            );
            (-1001, e)
        })?
    };
    let result = f(&param);
    debug!("invoke function, return {:?}", &result);
    let out_buf = unsafe { slice::from_raw_parts_mut(p2_ptr, p2_len) };

    let result_b = serialize_result(result).map_err(|e| (-2002, e))?;
    if result_b.len() > out_buf.len() {
        return Err((
            -2024,
            mudu_error!(
                ErrorCode::InsufficientBufferSpace,
                "insufficient buffer space"
            ),
        ));
    }
    out_buf[..result_b.len()].copy_from_slice(&result_b);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mudu::error::ErrorCode;
    use mudu::mudu_error;
    use mudu_type::data_value::DataValue;

    fn sample_param() -> ProcedureParam {
        ProcedureParam::new(
            42u128,
            7u64,
            vec![
                DataValue::from_i32(1),
                DataValue::from_string("x".to_string()),
            ],
        )
    }

    fn sample_result() -> ProcedureResult {
        ProcedureResult::new(vec![DataValue::from_i64(99)])
    }

    #[test]
    fn serialize_deserialize_param_roundtrip() {
        let param = sample_param();
        let expected_session = param.session_id();
        let expected_procedure = param.procedure_id();
        let expected_len = param.param_list().len();
        let bytes = serialize_param(param).unwrap();
        let decoded = deserialize_param(&bytes).unwrap();
        assert_eq!(decoded.session_id(), expected_session);
        assert_eq!(decoded.procedure_id(), expected_procedure);
        assert_eq!(decoded.param_list().len(), expected_len);
    }

    #[test]
    fn deserialize_param_empty_input_fails() {
        let err = deserialize_param(&[]).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
        assert!(err.message().contains("cannot deserialize param"));
    }

    #[test]
    fn serialize_deserialize_result_ok_roundtrip() {
        let result = Ok(sample_result());
        let bytes = serialize_result(result).unwrap();
        let decoded = deserialize_result(&bytes).unwrap();
        assert_eq!(decoded.return_list().len(), 1);
        assert_eq!(decoded.return_list()[0].to_i64(), 99);
    }

    #[test]
    fn serialize_deserialize_result_err_roundtrip() {
        let result: RS<ProcedureResult> = Err(mudu_error!(ErrorCode::Database, "db error"));
        let bytes = serialize_result(result).unwrap();
        let decoded = deserialize_result(&bytes);
        assert!(decoded.is_err());
        assert_eq!(decoded.unwrap_err().ec(), ErrorCode::Database);
    }

    #[test]
    fn invoke_procedure_sync_wrapper() {
        let param = sample_param();
        let param_bytes = serialize_param(param).unwrap();

        let output = invoke_procedure(param_bytes, |_p| {
            Ok(ProcedureResult::new(vec![DataValue::from_i32(123)]))
        });

        let decoded = deserialize_result(&output).unwrap();
        assert_eq!(decoded.return_list()[0].to_i32(), 123);
    }

    #[tokio::test]
    async fn invoke_procedure_async_wrapper() {
        let param = sample_param();
        let param_bytes = serialize_param(param).unwrap();

        let output = invoke_procedure_async(param_bytes, |_p| async {
            Ok(ProcedureResult::new(vec![DataValue::from_i32(456)]))
        })
        .await;

        let decoded = deserialize_result(&output).unwrap();
        assert_eq!(decoded.return_list()[0].to_i32(), 456);
    }

    #[test]
    fn invoke_wrapper_success() {
        let param = sample_param();
        let input = serialize_param(param).unwrap();
        let proc = |_p: &ProcedureParam| -> RS<ProcedureResult> {
            Ok(ProcedureResult::new(vec![DataValue::from_i32(777)]))
        };

        let mut output = vec![0u8; 256];
        let code = invoke_wrapper(
            input.as_ptr(),
            input.len(),
            output.as_mut_ptr(),
            output.len(),
            proc,
        );
        assert_eq!(code, 0);
        let decoded = deserialize_result(&output).unwrap();
        assert_eq!(decoded.return_list()[0].to_i32(), 777);
    }

    #[test]
    fn invoke_wrapper_deserialize_error() {
        let proc =
            |_p: &ProcedureParam| -> RS<ProcedureResult> { Ok(ProcedureResult::new(vec![])) };
        let mut output = vec![0u8; 64];
        let empty: Vec<u8> = vec![];
        let code = invoke_wrapper(
            empty.as_ptr(),
            empty.len(),
            output.as_mut_ptr(),
            output.len(),
            proc,
        );
        assert_eq!(code, -1001);
    }

    #[test]
    fn invoke_wrapper_insufficient_buffer() {
        let param = sample_param();
        let input = serialize_param(param).unwrap();
        let proc = |_p: &ProcedureParam| -> RS<ProcedureResult> {
            Ok(ProcedureResult::new(vec![DataValue::from_string(
                "too long".to_string(),
            )]))
        };

        let mut output = vec![0u8; 1];
        let code = invoke_wrapper(
            input.as_ptr(),
            input.len(),
            output.as_mut_ptr(),
            output.len(),
            proc,
        );
        assert_eq!(code, -2024);
    }
}
