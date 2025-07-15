use crate::tuple::dat_internal::DatInternal;
use std::cmp::Ordering;
use std::hash::Hasher;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub enum ErrCompare {
    ErrInternal(String),
}

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
