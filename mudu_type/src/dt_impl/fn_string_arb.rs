use crate::dat_type::DatType;
use crate::dat_value::DatValue;
use crate::dt_fn_arbitrary::FnArbitrary;
use crate::dt_impl::dt_create::create_string_type;
use crate::type_error::TyEC;
use crate::type_error::TyErr;
use arbitrary::{Arbitrary, Unstructured};
use test_utils::_arb_limit::_ARB_MAX_STRING_LEN;
use test_utils::_arb_string::_arbitrary_string;

pub fn param_len(ty: &DatType) -> Result<u32, TyErr> {
    if let Some(param) = ty.as_string_param() {
        Ok(param.length())
    } else {
        Err(TyErr::new(
            TyEC::FatalInternalError,
            "failed to get parameter of string type".to_string(),
        ))
    }
}

pub fn fn_char_arb_val(u: &mut Unstructured, param: &DatType) -> arbitrary::Result<DatValue> {
    let length = param_len(param).unwrap();
    let s = _arbitrary_string(u, length as usize)?;
    DatValue::from_datum(s, param).map_err(|_| arbitrary::Error::IncorrectFormat)
}

pub fn fn_char_arb_printable(u: &mut Unstructured, param: &DatType) -> arbitrary::Result<String> {
    let length = param_len(param).unwrap();
    let s = _arbitrary_string(u, length as usize)?;
    Ok(format!("\"{}\"", s))
}

pub fn fn_char_arb_dt_param(u: &mut Unstructured) -> arbitrary::Result<DatType> {
    let length = u32::arbitrary(u)?;
    let length = length % _ARB_MAX_STRING_LEN as u32;
    Ok(create_string_type(Some(length)))
}

pub const FN_CHAR_FIXED_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_char_arb_dt_param,
    value_object: fn_char_arb_val,
    value_print: fn_char_arb_printable,
};
