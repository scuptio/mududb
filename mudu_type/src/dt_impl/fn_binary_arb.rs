use crate::dat_textual::DatTextual;
use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dat_value::DatValue;
use crate::dt_fn_arbitrary::FnArbitrary;
use arbitrary::{Arbitrary, Unstructured};

pub fn fn_binary_arb_object(
    u: &mut Unstructured,
    _: &DatType,
) -> arbitrary::Result<DatValue> {
    let n = u8::arbitrary(u)? as usize;


    let mut vec = Vec::with_capacity(n);
    for _ in 0..n {
        let v = u8::arbitrary(u)?;
        vec.push(v);
    }
    Ok(DatValue::from_binary(vec))
}

pub fn fn_binary_arb_printable(
    u: &mut Unstructured,
    dat_type: &DatType,
) -> arbitrary::Result<String> {
    let object = fn_binary_arb_object(u, dat_type)?;
    let printable: DatTextual = DatTypeID::Binary.fn_output()(&object, dat_type).unwrap();
    Ok(printable.into())
}

pub fn fn_binary_arb_dt_param(_: &mut Unstructured) -> arbitrary::Result<DatType> {
    let dat_type = DatType::new_no_param(DatTypeID::Binary);
    Ok(dat_type)
}

pub const FN_BINARY_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_binary_arb_dt_param,
    value_object: fn_binary_arb_object,
    value_print: fn_binary_arb_printable,
};
