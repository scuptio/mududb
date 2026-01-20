use mudu::common::result::RS;

pub trait AttrBinary<T> {
    fn get_binary(&self, value: &T) -> RS<Vec<u8>>;

    fn set_binary<D: AsRef<[u8]>>(binary: D, value: &mut T) -> RS<()>;
}
