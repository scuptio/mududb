use serde::{Deserialize, Serialize};

#[derive(Copy, Eq, PartialEq, Ord, PartialOrd, Clone, Debug, Hash, Serialize, Deserialize)]
pub enum LenKind {
    FixedLen,
    VarLen,
}

impl LenKind {
    pub fn new(is_fixed: bool) -> Self {
        if is_fixed {
            Self::FixedLen
        } else {
            Self::VarLen
        }
    }
}
