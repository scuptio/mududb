use crate::common::result::RS;

pub trait AttrDatum {
    fn get_datum(&self) -> RS<Vec<u8>>;

    fn set_datum<D: AsRef<Vec<u8>>>(&mut self, datum: D) -> RS<()>;
}
