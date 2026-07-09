use crate::data_type::DataType;
use crate::data_type_fn_arbitrary::FnArbitrary;
use crate::data_type_impl::data_type_create::create_string_type;
use crate::data_value::DataValue;
use crate::type_error::TyEC;
use crate::type_error::TyErr;
use arbitrary::{Arbitrary, Unstructured};
use test_utils::_arb_limit::_ARB_MAX_STRING_LEN;
use test_utils::_arb_string::_arbitrary_string;

pub fn param_len(ty: &DataType) -> Result<u32, TyErr> {
    if let Some(param) = ty.as_string_param() {
        Ok(param.length())
    } else {
        Err(TyErr::new(
            TyEC::FatalInternalError,
            "failed to get parameter of string type".to_string(),
        ))
    }
}

pub fn fn_char_arb_val(u: &mut Unstructured, param: &DataType) -> arbitrary::Result<DataValue> {
    let length = param_len(param).unwrap();
    let s = _arbitrary_string(u, length as usize)?;
    DataValue::from_datum(s, param).map_err(|_| arbitrary::Error::IncorrectFormat)
}

pub fn fn_char_arb_printable(u: &mut Unstructured, param: &DataType) -> arbitrary::Result<String> {
    let length = param_len(param).unwrap();
    let s = _arbitrary_string(u, length as usize)?;
    serde_json::to_string(&s).map_err(|_| arbitrary::Error::IncorrectFormat)
}

pub fn fn_string_arb_data_type_param(u: &mut Unstructured) -> arbitrary::Result<DataType> {
    let length = u32::arbitrary(u)?;
    let length = length % _ARB_MAX_STRING_LEN as u32;
    Ok(create_string_type(Some(length)))
}

pub const FN_CHAR_FIXED_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_string_arb_data_type_param,
    value_object: fn_char_arb_val,
    value_print: fn_char_arb_printable,
};
