use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::dt_param::{ErrParam, FnParam, ParamObj};

pub fn fn_char_dt_param_in(params: &Vec<String>) -> Result<ParamObj, ErrParam> {
    let opt_param = params.get(0);
    if let Some(s_len) = opt_param {
        let len: u32 = s_len
            .parse()
            .map_err(|e| ErrParam::ParamParseError(format!("char length parse error {}", e)))?;
        Ok(ParamObj::from(DatTypeID::FixedLenString, params.clone(), len))
    } else {
        Err(ErrParam::ParamParseError(
            "char type only have 1 parameter".to_string(),
        ))
    }
}

pub const FN_CHAR_PARAM: FnParam = FnParam {
    input: fn_char_dt_param_in,
};
