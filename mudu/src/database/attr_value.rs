use crate::common::result::RS;
use crate::database::attr_datum::AttrDatum;
use crate::tuple::datum::Datum;

pub trait AttrValue<T: Datum>: AttrDatum + Sized {
    fn from_binary<B: AsRef<[u8]>>(datum: [u8]) -> RS<Self>;

    fn table_name() -> &'static str;

    fn column_name() -> &'static str;

    fn is_null(&self) -> bool;

    fn get_opt_value(&self) -> Option<T>;

    fn set_opt_value(&mut self, value: Option<T>);

    fn get_value(&self) -> T;

    fn set_value(&mut self, value: T);
}