use crate::common::result::RS;
use crate::database::attr_binary::AttrBinary;
use crate::tuple::datum::Datum;
use crate::tuple::datum_desc::DatumDesc;

pub trait AttrValue<T: Datum>: AttrBinary + Sized {
    fn datum_desc() -> &'static DatumDesc {
        T::datum_desc()
    }

    fn new(datum: T) -> Self;

    fn from_binary<B: AsRef<[u8]>>(datum: B) -> RS<Self>;

    fn table_name() -> &'static str;

    fn column_name() -> &'static str;

    fn get_value(&self) -> T;

    fn set_value(&mut self, value: T);
}
