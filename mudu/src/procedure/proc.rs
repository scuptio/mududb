use crate::common::result::RS;
use crate::common::serde_utils::{deserialize_sized_from, serialize_sized_to};
use crate::common::xid::XID;
use crate::error::err::MError;
use crate::procedure::proc_param::ProcParam;
use crate::procedure::proc_result::ProcResult;
use std::slice;
use tracing::{error, info};

pub fn invoke_proc(
    p1_ptr: *const u8,
    p1_len: usize,
    p2_ptr: *mut u8,
    p2_len: usize,
    proc: fn(xid: XID, vec: Vec<Vec<u8>>) -> RS<Vec<Vec<u8>>>,
) -> i32 {
    let r = _invoke_proc(p1_ptr, p1_len, p2_ptr, p2_len, proc);
    match r {
        Ok(()) => 0,
        Err((code, _e)) => code,
    }
}

fn _invoke_proc(
    p1_ptr: *const u8,
    p1_len: usize,
    p2_ptr: *mut u8,
    p2_len: usize,
    f: fn(xid: XID, vec: Vec<Vec<u8>>) -> RS<Vec<Vec<u8>>>,
) -> Result<(), (i32, MError)> {
    let param: ProcParam = unsafe {
        let slice = slice::from_raw_parts(p1_ptr, p1_len);
        let (param, _size) = deserialize_sized_from::<ProcParam>(slice).map_err(|e| {
            error!("deserialized error {}, length {}", e, p1_len);
            (-1001, e)
        })?;
        param
    };
    info!("invoke function, param {:?}", &param);
    let result = f(param.xid(), param.into_param_vec());
    info!("invoke function, return {:?}", &result);
    let out_buf = unsafe {
        let slice = slice::from_raw_parts_mut(p2_ptr, p2_len);
        slice
    };
    let proc_result = ProcResult::new(result);
    serialize_sized_to(&proc_result, out_buf).map_err(|e| (-2002, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::tuple::rs_tuple_datum::RsTupleDatum;
    #[test]
    fn test() {
        let tuple = (1i32, 2i64, "s".to_string());
        let desc = <(i32, i64, String)>::tuple_desc_static();
        println!("tuple {:?} and its describe {:?}", tuple, desc.fields());
    }
}

pub const MUDU_PROC_PREFIX: &'static str = "mudu_";
pub const MUDU_PROC_INNER_PREFIX: &'static str = "mudu_inner_";
pub const MUDU_PROC_ARGV_DESC_PREFIX: &'static str = "mudu_argv_desc_";
pub const MUDU_PROC_RESULT_DESC_PREFIX: &'static str = "mudu_result_desc_";
pub const MUDU_PROC_PROC_DESC_PREFIX: &'static str = "mudu_proc_desc_";
