use crate::common::result::RS;

pub trait ToResult<T>: Sized {
    fn to(self) -> RS<T>;
}

