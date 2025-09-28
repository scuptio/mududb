use crate::common::default_value;
use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::dt_param::{ErrParam, FnParam};
use crate::data_type::param_obj::ParamObj;

pub fn fn_char_dt_param_in(params: &Vec<String>) -> Result<ParamObj, ErrParam> {
    let opt_param = params.get(0);
    if let Some(s_len) = opt_param {
        let len: u32 = s_len
            .parse()
            .map_err(|e| ErrParam::ParamParseError(format!("char length parse error {}", e)))?;
        Ok(ParamObj::from(DatTypeID::CharFixedLen, params.clone(), len))
    } else {
        Err(ErrParam::ParamParseError(
            "char type only have 1 parameter".to_string(),
        ))
    }
}

pub fn fn_char_dt_param_default() -> ParamObj {
    ParamObj::from(
        DatTypeID::CharFixedLen,
        vec![default_value::DT_CHAR_FIXED_LEN_DEFAULT.to_string()],
        default_value::DT_CHAR_FIXED_LEN_DEFAULT,
    )
}

pub const FN_CHAR_FIXED_PARAM: FnParam = FnParam {
    input: fn_char_dt_param_in,
    default: fn_char_dt_param_default,
};
