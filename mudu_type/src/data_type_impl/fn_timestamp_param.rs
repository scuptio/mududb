use crate::data_type::DataType;
use crate::data_type_fn_param::FnParam;
use crate::data_type_impl::temporal::validate_timestamp_param;
use crate::data_type_param_timestamp::DataTypeParamTimestamp;
use crate::type_error::{TyEC, TyErr};

pub fn fn_timestamp_data_type_param_in(params: &str) -> Result<DataType, TyErr> {
    let param: DataTypeParamTimestamp = serde_json::from_str(params).map_err(|e| {
        TyErr::new(
            TyEC::ParamParseError,
            format!("parse timestamp parameter error {}", e),
        )
    })?;
    validate_timestamp_param(&param)?;
    Ok(DataType::from_timestamp(param))
}

pub fn fn_timestamp_data_type_param_default() -> DataType {
    DataType::from_timestamp(DataTypeParamTimestamp::default())
}

pub const FN_TIMESTAMP_PARAM: FnParam = FnParam {
    input: fn_timestamp_data_type_param_in,
    default: Some(fn_timestamp_data_type_param_default),
};
