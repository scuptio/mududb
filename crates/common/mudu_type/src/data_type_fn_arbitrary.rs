use crate::data_type::DataType;
use crate::data_value::DataValue;
use arbitrary::Unstructured;

/// Generates an arbitrary internal value of a type (test support).
pub type FnArbValue = fn(u: &mut Unstructured, &DataType) -> arbitrary::Result<DataValue>;

/// Generates an arbitrary printable value of a type (test support).
pub type FnArbPrintable = fn(u: &mut Unstructured, &DataType) -> arbitrary::Result<String>;

/// Generates an arbitrary type object of a family (test support).
pub type FnArbParam = fn(_u: &mut Unstructured) -> arbitrary::Result<DataType>;

/// The arbitrary value-generation function table of a data type family
/// (test support).
pub struct FnArbitrary {
    /// Generates an arbitrary type object of the family.
    pub param: FnArbParam,
    /// Generates an arbitrary internal value.
    pub value_object: FnArbValue,
    /// Generates an arbitrary printable value.
    pub value_print: FnArbPrintable,
}
