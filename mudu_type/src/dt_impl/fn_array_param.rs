use crate::dat_type::DatType;
use crate::dt_fn_param::FnParam;
use crate::dtp_array::DTPArray;
use crate::type_error::{TyEC, TyErr};

pub fn fn_array_param_in(params: &str) -> Result<DatType, TyErr> {
    let param = serde_json::from_str::<DTPArray>(params)
        .map_err(|err| TyErr::new(TyEC::ParamParseError, err.to_string()))?;
    Ok(DatType::from_array(param))
}

pub const FN_ARRAY_PARAM: FnParam = FnParam {
    input: fn_array_param_in,
    default: None,
};
