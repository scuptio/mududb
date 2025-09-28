use crate::common::result::RS;
use crate::tuple::tuple_item::TupleItem;

pub trait ResultSet: Send + Sync {
    fn next(&self) -> RS<Option<TupleItem>>;
}