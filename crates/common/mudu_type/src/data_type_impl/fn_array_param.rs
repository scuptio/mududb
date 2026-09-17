use crate::data_type::DataType;
use crate::data_type_fn_param::FnParam;
use crate::data_type_param_array::DataTypeParamArray;
use crate::type_error::{TyEC, TyErr};

pub fn fn_array_param_in(params: &str) -> Result<DataType, TyErr> {
    let param = serde_json::from_str::<DataTypeParamArray>(params)
        .map_err(|err| TyErr::new(TyEC::ParamParseError, err.to_string()))?;
    Ok(DataType::from_array(param))
}

pub const FN_ARRAY_PARAM: FnParam = FnParam {
    input: fn_array_param_in,
    default: None,
};
