use crate::dat_type::DatType;
use crate::dat_value::DatValue;
use arbitrary::Unstructured;

pub type FnArbValue = fn(u: &mut Unstructured, &DatType) -> arbitrary::Result<DatValue>;

pub type FnArbPrintable = fn(u: &mut Unstructured, &DatType) -> arbitrary::Result<String>;

pub type FnArbParam = fn(_u: &mut Unstructured) -> arbitrary::Result<DatType>;

pub struct FnArbitrary {
    pub param: FnArbParam,
    pub value_object: FnArbValue,
    pub value_print: FnArbPrintable,
}
