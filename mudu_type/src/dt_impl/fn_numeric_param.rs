use crate::dat_type::DatType;
use crate::dt_fn_param::FnParam;
use crate::dtp_numeric::DTPNumeric;
use crate::type_error::{TyEC, TyErr};

pub fn fn_numeric_dt_param_in(params: &str) -> Result<DatType, TyErr> {
    let param: DTPNumeric = serde_json::from_str(params).map_err(|e| {
        TyErr::new(
            TyEC::ParamParseError,
            format!("parse numeric parameter error {}", e),
        )
    })?;
    validate_numeric_param(&param)?;
    Ok(DatType::from_numeric(param))
}

pub fn fn_numeric_dt_param_default() -> DatType {
    DatType::from_numeric(DTPNumeric::default())
}

fn validate_numeric_param(param: &DTPNumeric) -> Result<(), TyErr> {
    param
        .validate()
        .map_err(|message| TyErr::new(TyEC::ParamParseError, message))
}

pub const FN_NUMERIC_PARAM: FnParam = FnParam {
    input: fn_numeric_dt_param_in,
    default: Some(fn_numeric_dt_param_default),
};
