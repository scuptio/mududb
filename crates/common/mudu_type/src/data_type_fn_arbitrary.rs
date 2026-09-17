use crate::data_type::DataType;
use crate::data_value::DataValue;
use arbitrary::Unstructured;

pub type FnArbValue = fn(u: &mut Unstructured, &DataType) -> arbitrary::Result<DataValue>;

pub type FnArbPrintable = fn(u: &mut Unstructured, &DataType) -> arbitrary::Result<String>;

pub type FnArbParam = fn(_u: &mut Unstructured) -> arbitrary::Result<DataType>;

pub struct FnArbitrary {
    pub param: FnArbParam,
    pub value_object: FnArbValue,
    pub value_print: FnArbPrintable,
}
