use crate::tuple::dat_internal::DatInternal;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::hash::Hasher;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone,
    Serialize, Deserialize
)]
pub enum ErrCompare {
    ErrInternal(String),
}

impl Display for ErrCompare {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)?;
        Ok(())
    }
}

impl Error for ErrCompare {}

pub type FnHash = fn(&DatInternal, &mut dyn Hasher) -> Result<(), ErrCompare>;

/// `FnOrder` returns ordering result of a comparison between two internal values.
pub type FnOrder = fn(&DatInternal, &DatInternal) -> Result<Ordering, ErrCompare>;

/// `FnEqual` return equal result of a comparison between two internal values.
pub type FnEqual = fn(&DatInternal, &DatInternal) -> Result<bool, ErrCompare>;

pub struct FnCompare {
    pub order: FnOrder,
    pub equal: FnEqual,
    pub hash: FnHash,
}
