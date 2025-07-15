use crate::common::result::RS;
use crate::database::row_desc::RowDesc;
use crate::database::tuple_row::TupleRow;
use crate::tuple::datum::Datum;

pub trait Record : Sized {
    fn table_name() -> &'static str;

    fn from_tuple<T:AsRef<TupleRow>, D:AsRef<RowDesc>>(tuple_row:T, row_desc:D) -> RS<Self>;

    fn to_tuple<D:AsRef<RowDesc>>(&self, row_desc:D) -> RS<TupleRow>;
    
    fn get(&self, field_name:&str) -> RS<Option<Datum>>;

    fn set<D:AsRef<Datum>>(&mut self, field_name:&str, datum:Option<D>) -> RS<()>;
}



