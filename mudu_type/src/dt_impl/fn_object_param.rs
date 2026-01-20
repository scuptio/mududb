use crate::dat_type::DatType;
use crate::dt_fn_param::FnParam;
use crate::dtp_object::DTPRecord;
use crate::type_error::{TyEC, TyErr};
use mudu::utils;

pub fn fn_object_param_in(s: &str) -> Result<DatType, TyErr> {
    let param: DTPRecord = utils::json::from_json_str(s).map_err(|_e| {
        TyErr::new(
            TyEC::ParamParseError,
            "parse parameter json error".to_string(),
        )
    })?;
    Ok(DatType::from_record(param))
}

pub const FN_OBJECT_PARAM: FnParam = FnParam {
    input: fn_object_param_in,
    default: None,
};
