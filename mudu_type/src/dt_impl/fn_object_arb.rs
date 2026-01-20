use crate::dat_type::DatType;
use crate::dat_value::DatValue;
use crate::dt_fn_arbitrary::FnArbitrary;
use arbitrary::Unstructured;

pub fn fn_object_arb_typed(_: &mut Unstructured, _: &DatType) -> arbitrary::Result<DatValue> {
    todo!()
}

pub fn fn_object_arb_printable(_: &mut Unstructured, _: &DatType) -> arbitrary::Result<String> {
    todo!()
}

pub fn fn_object_arb_dt_param(_: &mut Unstructured) -> arbitrary::Result<DatType> {
    todo!()
}

pub const FN_OBJECT_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_object_arb_dt_param,
    value_object: fn_object_arb_typed,
    value_print: fn_object_arb_printable,
};
