use crate::data_type::DataType;
use crate::data_type_fn_arbitrary::FnArbitrary;
use crate::data_type_impl::data_type_create::create_string_type;
use crate::data_value::DataValue;
use crate::type_error::TyEC;
use crate::type_error::TyErr;
use arbitrary::{Arbitrary, Unstructured};

/// Maximum length of an arbitrary string.
const _ARB_MAX_STRING_LEN: usize = 100;

/// Generates an arbitrary string whose length is bounded by `len`.
fn _arbitrary_string(u: &mut Unstructured, len: usize) -> arbitrary::Result<String> {
    if len == 0 {
        Ok(String::new())
    } else {
        let v = u32::arbitrary(u)?;
        let str_len = (v as usize) % len;
        let d = u.bytes(str_len)?;
        let uu = Unstructured::new(d);
        let name = String::arbitrary_take_rest(uu)?;
        Ok(name)
    }
}

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
