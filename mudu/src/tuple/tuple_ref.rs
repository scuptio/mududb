use crate::common::error::ER;
use crate::common::result::RS;
use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::tuple::read_datum::{read_fixed_len_value, read_var_len_value};
use crate::tuple::slot::Slot;
use crate::tuple::tuple_desc::TupleDesc;

pub struct TupleRef<'a, 'b> {
    tuple: &'a [u8],
    desc: &'b TupleDesc,
}

impl<'a, 'b> TupleRef<'a, 'b> {
    pub fn new(tuple: &'a [u8], desc: &'b TupleDesc) -> TupleRef<'a, 'b> {
        Self { tuple, desc }
    }
    pub fn columns(&self) -> usize {
        self.desc.field_count()
    }
    pub fn get_tuple(&self) -> &'a [u8] {
        self.tuple
    }

    pub fn get_binary_data(&self, idx: usize) -> RS<&'a [u8]> {
        let fd = self.desc.get_field_desc(idx);
        self._get_binary_data(fd.slot(), fd.is_fixed_len())
    }

    pub fn get_typed_value(&self, idx: usize) -> RS<DatTyped> {
        let fd = self.desc.get_field_desc(idx);
        let binary = self._get_binary_data(fd.slot(), fd.is_fixed_len())?;
        let data_type = fd.data_type();
        let recv = data_type.fn_recv();
        let to_typed = data_type.fn_to_typed();
        let internal = recv(binary, fd.type_param()).map_err(ER::ConvertErr)?;
        let typed_value = to_typed(&internal, fd.type_param()).map_err(ER::ConvertErr)?;
        Ok(typed_value)
    }

    fn _get_binary_data(&self, s: &Slot, fixed_len: bool) -> RS<&'a [u8]> {
        if fixed_len {
            read_fixed_len_value(s.offset(), s.length(), self.tuple)
        } else {
            read_var_len_value(s.offset(), self.tuple)
        }
    }
}
