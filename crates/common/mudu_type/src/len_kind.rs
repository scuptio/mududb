use serde::{Deserialize, Serialize};

/// Whether a data type has a fixed or variable binary length.
#[derive(Copy, Eq, PartialEq, Ord, PartialOrd, Clone, Debug, Hash, Serialize, Deserialize)]
pub enum LenKind {
    /// The type always occupies the same number of bytes.
    FixedLen,
    /// The encoded length varies per value.
    VarLen,
}

impl LenKind {
    /// Returns `FixedLen` when `is_fixed` is true, `VarLen` otherwise.
    pub fn new(is_fixed: bool) -> Self {
        if is_fixed {
            Self::FixedLen
        } else {
            Self::VarLen
        }
    }
}
