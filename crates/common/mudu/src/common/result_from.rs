use crate::common::result::RS;

pub trait ResultFrom<T>: Sized {
    fn from(value: T) -> RS<Self>;
}
