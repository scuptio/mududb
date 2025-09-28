pub use crate::data_type::param_obj::ParamObj;
use std::fmt;

#[derive(Debug)]
pub enum ErrParam {
    ParamParseError(String),
}
#[derive(Clone, Debug)]
pub struct FnParam {
    pub input: FnParamIn,
    pub default: FnParamDefault,
}

pub type FnParamIn = fn(params: &Vec<String>) -> Result<ParamObj, ErrParam>;

pub type FnParamDefault = fn() -> ParamObj;

impl fmt::Display for FnParam {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

