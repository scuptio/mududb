use crate::common::result::RS;

pub trait AttrBinary {
    fn get_binary(&self) -> RS<Vec<u8>>;

    fn set_binary<D: AsRef<[u8]>>(&mut self, datum: D) -> RS<()>;
}
