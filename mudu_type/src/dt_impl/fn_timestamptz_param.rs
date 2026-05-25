use crate::dat_type::DatType;
use crate::dt_fn_param::FnParam;
use crate::dt_impl::temporal::validate_timestamptz_param;
use crate::dtp_timestamptz::DTPTimestampTz;
use crate::type_error::{TyEC, TyErr};

pub fn fn_timestamptz_dt_param_in(params: &str) -> Result<DatType, TyErr> {
    let param: DTPTimestampTz = serde_json::from_str(params).map_err(|e| {
        TyErr::new(
            TyEC::ParamParseError,
            format!("parse timestamptz parameter error {}", e),
        )
    })?;
    validate_timestamptz_param(&param)?;
    Ok(DatType::from_timestamptz(param))
}

pub fn fn_timestamptz_dt_param_default() -> DatType {
    DatType::from_timestamptz(DTPTimestampTz::default())
}

pub const FN_TIMESTAMPTZ_PARAM: FnParam = FnParam {
    input: fn_timestamptz_dt_param_in,
    default: Some(fn_timestamptz_dt_param_default),
};
