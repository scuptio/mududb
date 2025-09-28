use crate::common::default_value;
use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::dt_impl::fn_char_fixed_param::fn_char_dt_param_in;
use crate::data_type::dt_param::FnParam;
use crate::data_type::param_obj::ParamObj;

pub fn fn_varchar_dt_param_default() -> ParamObj {
    ParamObj::from(
        DatTypeID::CharFixedLen,
        vec![default_value::DT_CHAR_VAR_LEN_DEFAULT.to_string()],
        default_value::DT_CHAR_VAR_LEN_DEFAULT,
    )
}

pub const FN_VARCHAR_PARAM: FnParam = FnParam {
    input: fn_char_dt_param_in,
    default: fn_varchar_dt_param_default,
};
