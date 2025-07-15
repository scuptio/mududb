use crate::data_type::dt_fn_base::FnBase;
use crate::data_type::dt_fn_compare::FnCompare;
use crate::data_type::dt_impl::fn_char;
use crate::data_type::dt_param::ParamObj;

pub fn fn_varchar_len(_: &ParamObj) -> Option<usize> {
    None
}

pub const FN_VAR_LEN_STRING_COMPARE: FnCompare = FnCompare {
    order: fn_char::fn_char_order,
    equal: fn_char::fn_char_equal,
    hash: fn_char::fn_char_hash,
};

pub const FN_VAR_LEN_STRING_CONVERT: FnBase = FnBase {
    input: fn_char::fn_char_in,
    output: fn_char::fn_char_out,
    len: fn_varchar_len,
    recv: fn_char::fn_char_recv,
    send: fn_char::fn_char_send,
    send_to: fn_char::fn_char_send_to,
    to_typed: fn_char::fn_char_to_typed,
    from_typed: fn_char::fn_char_from_typed,
};
