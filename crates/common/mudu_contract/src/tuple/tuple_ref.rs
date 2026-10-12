//! Borrowed accessor over a binary tuple.

use crate::tuple::read_datum::{read_fixed_len_value, read_var_len_value};
use crate::tuple::slot::Slot;
use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
use mudu::common::result::RS;

/// A borrowed view over a binary tuple, decoding fields via its layout descriptor.
pub struct TupleRef<'a, 'b> {
    tuple: &'a [u8],
    desc: &'b TupleBinaryDesc,
}

impl<'a, 'b> TupleRef<'a, 'b> {
    /// Creates a view over `tuple` bytes laid out by `desc`.
    pub fn new(tuple: &'a [u8], desc: &'b TupleBinaryDesc) -> TupleRef<'a, 'b> {
        Self { tuple, desc }
    }

    /// Returns the number of fields in the tuple.
    pub fn columns(&self) -> usize {
        self.desc.field_count()
    }

    /// Returns the raw tuple bytes.
    pub fn get_tuple(&self) -> &'a [u8] {
        self.tuple
    }

    /// Extracts the binary value of field `idx`.
    pub fn get_binary_data(&self, idx: usize) -> RS<&'a [u8]> {
        let fd = self.desc.get_field_desc(idx);
        self._get_binary_data(fd.slot(), fd.is_fixed_len())
    }

    fn _get_binary_data(&self, s: &Slot, fixed_len: bool) -> RS<&'a [u8]> {
        if fixed_len {
            read_fixed_len_value(s.offset(), s.length(), self.tuple)
        } else {
            read_var_len_value(s.offset(), self.tuple)
        }
    }
}
