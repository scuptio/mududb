use crate::data_type::DataType;
use crate::data_type_fn_param::FnParam;
use crate::data_type_impl::temporal::validate_time_param;
use crate::data_type_param_time::DataTypeParamTime;
use crate::type_error::{TyEC, TyErr};

pub fn fn_time_data_type_param_in(params: &str) -> Result<DataType, TyErr> {
    let param: DataTypeParamTime = serde_json::from_str(params).map_err(|e| {
        TyErr::new(
            TyEC::ParamParseError,
            format!("parse time parameter error {}", e),
        )
    })?;
    validate_time_param(&param)?;
    Ok(DataType::from_time(param))
}

pub fn fn_time_data_type_param_default() -> DataType {
    DataType::from_time(DataTypeParamTime::default())
}

pub const FN_TIME_PARAM: FnParam = FnParam {
    input: fn_time_data_type_param_in,
    default: Some(fn_time_data_type_param_default),
};
