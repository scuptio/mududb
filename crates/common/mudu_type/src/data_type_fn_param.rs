pub use crate::data_type::DataType;
use crate::type_error::TyErr;
use std::fmt;

/// The parameter function table of a parameterized data type family.
#[derive(Clone, Debug)]
pub struct FnParam {
    /// Parses a JSON parameter string into a concrete type object.
    pub input: FnParamIn,
    /// Produces the type object with the default parameter, if the family
    /// defines one.
    pub default: Option<FnParamDefault>,
}

/// Parses a JSON parameter string into a concrete type object.
pub type FnParamIn = fn(params: &str) -> Result<DataType, TyErr>;

/// Produces the type object with the default parameter.
pub type FnParamDefault = fn() -> DataType;

impl fmt::Display for FnParam {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}
