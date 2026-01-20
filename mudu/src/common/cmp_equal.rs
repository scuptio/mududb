use crate::error::err::MError;

pub trait Equal {
    type Error = MError;

    fn cmp_eq(&self, other: &Self) -> Result<bool, Self::Error>;
}
