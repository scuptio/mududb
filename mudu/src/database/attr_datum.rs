use crate::common::result::RS;
use crate::tuple::datum::Datum;

pub trait AttrDatum {
    fn get_datum(&self) -> RS<Datum>;

    fn set_datum<D:AsRef<Datum>>(&mut self, datum: D) -> RS<()>;
}