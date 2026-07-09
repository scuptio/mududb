use crate::data_type::DataType;
use crate::data_type_fn_param::FnParam;
use crate::data_type_param_record::DataTypeParamRecord;
use crate::type_error::{TyEC, TyErr};
use mudu::utils;

pub fn fn_object_param_in(s: &str) -> Result<DataType, TyErr> {
    let param: DataTypeParamRecord = utils::json::from_json_str(s).map_err(|_e| {
        TyErr::new(
            TyEC::ParamParseError,
            "parse parameter json error".to_string(),
        )
    })?;
    Ok(DataType::from_record(param))
}

pub const FN_OBJECT_PARAM: FnParam = FnParam {
    input: fn_object_param_in,
    default: None,
};
