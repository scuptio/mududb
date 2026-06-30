use crate::common::result::RS;

pub trait Equal {
    fn cmp_eq(&self, other: &Self) -> RS<bool>;
}
