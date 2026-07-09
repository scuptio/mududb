pub use crate::data_type::DataType;
use crate::type_error::TyErr;
use std::fmt;

#[derive(Clone, Debug)]
pub struct FnParam {
    pub input: FnParamIn,
    pub default: Option<FnParamDefault>,
}

pub type FnParamIn = fn(params: &str) -> Result<DataType, TyErr>;

pub type FnParamDefault = fn() -> DataType;

impl fmt::Display for FnParam {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}
