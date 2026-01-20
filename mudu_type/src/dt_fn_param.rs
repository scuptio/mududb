pub use crate::dat_type::DatType;
use crate::type_error::TyErr;
use std::fmt;

#[derive(Clone, Debug)]
pub struct FnParam {
    pub input: FnParamIn,
    pub default: Option<FnParamDefault>,
}

pub type FnParamIn = fn(params: &str) -> Result<DatType, TyErr>;

pub type FnParamDefault = fn() -> DatType;

impl fmt::Display for FnParam {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}
