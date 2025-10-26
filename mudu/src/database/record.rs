use crate::common::result::RS;
use crate::tuple::tuple_field::TupleField;
use crate::tuple::tuple_field_desc::TupleFieldDesc;

pub trait Record: Sized {
    fn new_empty() -> Self;

    fn tuple_desc() -> &'static TupleFieldDesc;

    fn table_name() -> &'static str;

    fn from_tuple<T: AsRef<TupleField>, D: AsRef<TupleFieldDesc>>(
        tuple_row: T,
        row_desc: D,
    ) -> RS<Self>;

    fn to_tuple<D: AsRef<TupleFieldDesc>>(&self, row_desc: D) -> RS<TupleField>;

    fn get_binary(&self, field_name: &str) -> RS<Option<Vec<u8>>>;

    fn set_binary<B: AsRef<[u8]>>(&mut self, field_name: &str, binary: B) -> RS<()>;
}
