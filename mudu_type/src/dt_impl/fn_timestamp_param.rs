use crate::dat_type::DatType;
use crate::dt_fn_param::FnParam;
use crate::dt_impl::temporal::validate_timestamp_param;
use crate::dtp_timestamp::DTPTimestamp;
use crate::type_error::{TyEC, TyErr};

pub fn fn_timestamp_dt_param_in(params: &str) -> Result<DatType, TyErr> {
    let param: DTPTimestamp = serde_json::from_str(params).map_err(|e| {
        TyErr::new(
            TyEC::ParamParseError,
            format!("parse timestamp parameter error {}", e),
        )
    })?;
    validate_timestamp_param(&param)?;
    Ok(DatType::from_timestamp(param))
}

pub fn fn_timestamp_dt_param_default() -> DatType {
    DatType::from_timestamp(DTPTimestamp::default())
}

pub const FN_TIMESTAMP_PARAM: FnParam = FnParam {
    input: fn_timestamp_dt_param_in,
    default: Some(fn_timestamp_dt_param_default),
};
