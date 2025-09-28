use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::data_type::param_obj::ParamObj;
use crate::tuple::dat_binary::DatBinary;
use crate::tuple::dat_internal::DatInternal;
use crate::tuple::dat_printable::DatPrintable;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone,
    Serialize, Deserialize
)]
pub enum ErrConvert {
    ErrTypeConvert(String),
    ErrLowBufSpace(usize),
}

impl Display for ErrConvert {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!("{:?}", self).as_str())
    }
}

impl Error for ErrConvert {}

/// `FnInput` converts the type's external textual representation to the internal representation
/// used by the operators and functions defined for the type.
pub type FnInput = fn(&DatPrintable, &ParamObj) -> Result<DatInternal, ErrConvert>;

/// `FnOutput` converts the type's the internal representation  used by the operators and functions
/// defined for the type to external textual representation.
pub type FnOutput = fn(&DatInternal, &ParamObj) -> Result<DatPrintable, ErrConvert>;

/// `FnLen` return the length of the type, if it is a fixed length type
pub type FnLen = fn(&ParamObj) -> Option<usize>;

/// `FnSend` converts from the internal representation to the external binary representation
pub type FnSend = fn(&DatInternal, &ParamObj) -> Result<DatBinary, ErrConvert>;

pub type FnSendTo = fn(&DatInternal, &ParamObj, &mut [u8]) -> Result<usize, ErrConvert>;

/// `FnRecv` converts from the external binary representation to the internal representation
pub type FnRecv = fn(&[u8], &ParamObj) -> Result<DatInternal, ErrConvert>;

pub type FnToTyped = fn(&DatInternal, &ParamObj) -> Result<DatTyped, ErrConvert>;

pub type FnFromTyped = fn(&DatTyped, &ParamObj) -> Result<DatInternal, ErrConvert>;


pub struct FnConvert {
    pub input: FnInput,
    pub output: FnOutput,
    pub len: FnLen,
    pub recv: FnRecv,
    pub send: FnSend,
    pub send_to: FnSendTo,
    pub to_typed: FnToTyped,
    pub from_typed: FnFromTyped,
}
