use crate::data_type::DataType;
use crate::data_type_fn_param::FnParam;
use crate::data_type_param_string::DataTypeParamString;
use crate::type_error::{TyEC, TyErr};
use mudu::common::default_value;

pub fn fn_string_data_type_param_in(params: &str) -> Result<DataType, TyErr> {
    let param: DataTypeParamString = serde_json::from_str(params).map_err(|e| {
        TyErr::new(
            TyEC::ParamParseError,
            format!("parse parameter error {}", e),
        )
    })?;
    Ok(DataType::from_string(param))
}

pub fn fn_string_data_type_param_default() -> DataType {
    let param = DataTypeParamString::new(default_value::DT_CHAR_FIXED_LEN_DEFAULT as u32);
    DataType::from_string(param)
}

pub const FN_CHAR_FIXED_PARAM: FnParam = FnParam {
    input: fn_string_data_type_param_in,
    default: Some(fn_string_data_type_param_default),
};
