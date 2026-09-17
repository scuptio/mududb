use crate::data_type::DataType;
use crate::data_type_fn_param::FnParam;
use crate::data_type_impl::temporal::validate_timestamptz_param;
use crate::data_type_param_timestamptz::DataTypeParamTimestampTz;
use crate::type_error::{TyEC, TyErr};

pub fn fn_timestamptz_data_type_param_in(params: &str) -> Result<DataType, TyErr> {
    let param: DataTypeParamTimestampTz = serde_json::from_str(params).map_err(|e| {
        TyErr::new(
            TyEC::ParamParseError,
            format!("parse timestamptz parameter error {}", e),
        )
    })?;
    validate_timestamptz_param(&param)?;
    Ok(DataType::from_timestamptz(param))
}

pub fn fn_timestamptz_data_type_param_default() -> DataType {
    DataType::from_timestamptz(DataTypeParamTimestampTz::default())
}

pub const FN_TIMESTAMPTZ_PARAM: FnParam = FnParam {
    input: fn_timestamptz_data_type_param_in,
    default: Some(fn_timestamptz_data_type_param_default),
};
