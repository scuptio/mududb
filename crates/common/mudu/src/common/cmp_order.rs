use crate::common::result::RS;
use std::cmp::Ordering;

pub trait Order {
    fn cmp_ord(&self, other: &Self) -> RS<Ordering>;
}
