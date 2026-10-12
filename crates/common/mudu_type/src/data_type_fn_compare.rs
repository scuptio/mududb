use crate::data_value::DataValue;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::hash::Hasher;

/// Error of a comparison, ordering, or hashing function.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Serialize, Deserialize)]
pub enum ErrCompare {
    /// An internal comparison failure with a message.
    ErrInternal(String),
}

impl Display for ErrCompare {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)?;
        Ok(())
    }
}

impl Error for ErrCompare {}

/// `FnHash` hashes an internal value into a [`Hasher`].
pub type FnHash = fn(&DataValue, &mut dyn Hasher) -> Result<(), ErrCompare>;

/// `FnOrder` returns ordering result of a comparison between two internal values.
pub type FnOrder = fn(&DataValue, &DataValue) -> Result<Ordering, ErrCompare>;

/// `FnEqual` return equal result of a comparison between two internal values.
pub type FnEqual = fn(&DataValue, &DataValue) -> Result<bool, ErrCompare>;

/// The comparison function table of a data type family.
pub struct FnCompare {
    /// Ordering comparison function.
    pub order: FnOrder,
    /// Equality comparison function.
    pub equal: FnEqual,
    /// Hashing function.
    pub hash: FnHash,
}
