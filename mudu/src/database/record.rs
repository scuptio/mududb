use crate::common::result::RS;
use crate::tuple::tuple_item::TupleItem;
use crate::tuple::tuple_item_desc::TupleItemDesc;

pub trait Record: Sized {
    fn tuple_desc() -> &'static TupleItemDesc;

    fn table_name() -> &'static str;

    fn from_tuple<T: AsRef<TupleItem>, D: AsRef<TupleItemDesc>>(tuple_row: T, row_desc: D) -> RS<Self>;

    fn to_tuple<D: AsRef<TupleItemDesc>>(&self, row_desc: D) -> RS<TupleItem>;

    fn get(&self, field_name: &str) -> RS<Option<Vec<u8>>>;

    fn set<D: AsRef<[u8]>>(&mut self, field_name: &str, datum: Option<D>) -> RS<()>;
}



