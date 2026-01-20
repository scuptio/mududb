use crate::dat_type::DatType;
use crate::dt_fn_param::FnParam;
use crate::dtp_string::DTPString;
use crate::type_error::{TyEC, TyErr};
use mudu::common::default_value;

pub fn fn_char_dt_param_in(params: &str) -> Result<DatType, TyErr> {
    let param: DTPString = serde_json::from_str(params).map_err(|e| {
        TyErr::new(
            TyEC::ParamParseError,
            format!("parse parameter error {}", e),
        )
    })?;
    Ok(DatType::from_string(param))
}

pub fn fn_char_dt_param_default() -> DatType {
    let param = DTPString::new(default_value::DT_CHAR_FIXED_LEN_DEFAULT as u32);
    DatType::from_string(param)
}

pub const FN_CHAR_FIXED_PARAM: FnParam = FnParam {
    input: fn_char_dt_param_in,
    default: Some(fn_char_dt_param_default),
};
