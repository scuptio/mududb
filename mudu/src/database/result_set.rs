use crate::common::result::RS;
use crate::tuple::tuple_field::TupleField;

pub trait ResultSet: Send + Sync {
    fn next(&self) -> RS<Option<TupleField>>;
}
