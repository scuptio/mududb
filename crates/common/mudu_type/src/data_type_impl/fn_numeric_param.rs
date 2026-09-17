use crate::data_type::DataType;
use crate::data_type_fn_param::FnParam;
use crate::data_type_param_numeric::DataTypeParamNumeric;
use crate::type_error::{TyEC, TyErr};

pub fn fn_numeric_data_type_param_in(params: &str) -> Result<DataType, TyErr> {
    let param: DataTypeParamNumeric = serde_json::from_str(params).map_err(|e| {
        TyErr::new(
            TyEC::ParamParseError,
            format!("parse numeric parameter error {}", e),
        )
    })?;
    validate_numeric_param(&param)?;
    Ok(DataType::from_numeric(param))
}

pub fn fn_numeric_data_type_param_default() -> DataType {
    DataType::from_numeric(DataTypeParamNumeric::default())
}

fn validate_numeric_param(param: &DataTypeParamNumeric) -> Result<(), TyErr> {
    param
        .validate()
        .map_err(|message| TyErr::new(TyEC::ParamParseError, message))
}

pub const FN_NUMERIC_PARAM: FnParam = FnParam {
    input: fn_numeric_data_type_param_in,
    default: Some(fn_numeric_data_type_param_default),
};
