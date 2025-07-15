use crate::common::result::RS;
use crate::database::tuple_row::TupleRow;

pub trait ResultSet {
    fn next(&self) -> RS<Option<TupleRow>>;
}