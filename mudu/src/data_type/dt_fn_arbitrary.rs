use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::data_type::dt_param::ParamObj;
use arbitrary::Unstructured;

pub type FnArbValue = fn(u: &mut Unstructured, _p: &ParamObj) -> arbitrary::Result<DatTyped>;

pub type FnArbPrintable = fn(u: &mut Unstructured, _p: &ParamObj) -> arbitrary::Result<String>;

pub type FnArbParam = fn(_u: &mut Unstructured) -> arbitrary::Result<ParamObj>;

pub struct FnArbitrary {
    pub param: FnArbParam,
    pub value_typed: FnArbValue,
    pub value_print: FnArbPrintable,
}
