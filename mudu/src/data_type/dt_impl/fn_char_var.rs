use crate::data_type::dt_fn_compare::FnCompare;
use crate::data_type::dt_fn_convert::FnConvert;
use crate::data_type::dt_impl::fn_char_fixed;
use crate::data_type::param_obj::ParamObj;

pub fn fn_varchar_len(_: &ParamObj) -> Option<usize> {
    None
}

pub const FN_CHAR_VAR_COMPARE: FnCompare = FnCompare {
    order: fn_char_fixed::fn_char_order,
    equal: fn_char_fixed::fn_char_equal,
    hash: fn_char_fixed::fn_char_hash,
};

pub const FN_CHAR_VAR_CONVERT: FnConvert = FnConvert {
    input: fn_char_fixed::fn_char_in,
    output: fn_char_fixed::fn_char_out,
    len: fn_varchar_len,
    recv: fn_char_fixed::fn_char_recv,
    send: fn_char_fixed::fn_char_send,
    send_to: fn_char_fixed::fn_char_send_to,
    to_typed: fn_char_fixed::fn_char_to_typed,
    from_typed: fn_char_fixed::fn_char_from_typed,
};
