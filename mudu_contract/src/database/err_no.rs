use lazy_static::lazy_static;
use std::collections::HashMap;

pub const EN_OK: i32 = 0;
pub const EN_DECODE_PARAM: i32 = 1;
pub const EN_INVOKE: i32 = 2;
pub const EN_INSUFFICIENT_BUFFER_LENGTH_FOR_OUTPUT: i32 = 3;
pub const EN_NO_OUTPUT_MEMORY: i32 = 3;
pub const EN_ENCODE_RESULT: i32 = 4;

lazy_static! {
    static ref ERR_MSG: HashMap<i32, &'static str> = HashMap::from([
        (EN_DECODE_PARAM, "encode parameter error"),
        (EN_INVOKE, "invoke procedure error"),
        (
            EN_INSUFFICIENT_BUFFER_LENGTH_FOR_OUTPUT,
            "insufficient buffer length for output error"
        ),
        (EN_NO_OUTPUT_MEMORY, "memory error"),
        (EN_ENCODE_RESULT, "encode result error"),
    ]);
}

pub fn errno_to_msg(errno: i32) -> String {
    ERR_MSG
        .get(&errno)
        .map_or(format!("no such error number {}", errno), |s| s.to_string())
}
