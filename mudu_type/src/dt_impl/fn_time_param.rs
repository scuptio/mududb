use crate::dat_type::DatType;
use crate::dt_fn_param::FnParam;
use crate::dt_impl::temporal::validate_time_param;
use crate::dtp_time::DTPTime;
use crate::type_error::{TyEC, TyErr};

pub fn fn_time_dt_param_in(params: &str) -> Result<DatType, TyErr> {
    let param: DTPTime = serde_json::from_str(params).map_err(|e| {
        TyErr::new(
            TyEC::ParamParseError,
            format!("parse time parameter error {}", e),
        )
    })?;
    validate_time_param(&param)?;
    Ok(DatType::from_time(param))
}

pub fn fn_time_dt_param_default() -> DatType {
    DatType::from_time(DTPTime::default())
}

pub const FN_TIME_PARAM: FnParam = FnParam {
    input: fn_time_dt_param_in,
    default: Some(fn_time_dt_param_default),
};
