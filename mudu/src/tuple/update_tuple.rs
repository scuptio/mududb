use crate::common::buf::Buf;
use crate::common::result::RS;
use crate::common::update_delta::UpdateDelta;
use crate::error::ec::EC;
use crate::m_error;
use crate::tuple::read_datum::{read_binary_data, read_data_capacity, read_slot};
use crate::tuple::slot::Slot;
use crate::tuple::tuple_binary::TupleSlice;
use crate::tuple::tuple_binary_desc::TupleBinaryDesc;

pub fn update_tuple(
    index: usize,
    value: &Buf,
    tuple_desc: &TupleBinaryDesc,
    tuple: &TupleSlice,
    delta: &mut Vec<UpdateDelta>,
) -> RS<()> {
    if index >= tuple_desc.field_count() {
        Err(m_error!(
            EC::InternalErr,
            format!(
                "tuple_slice index {} out of bounds, the maximum size {}",
                index,
                tuple_desc.field_count()
            )
        ))?;
    }

    let field = tuple_desc.get_field_desc(index);

    if field.is_fixed_len() {
        let slot = field.slot();
        let update = UpdateDelta::new(slot.offset() as u32, slot.length() as u32, value.clone());
        delta.push(update);
    } else {
        let slot = read_slot(field, tuple)?;
        let capacity = read_data_capacity(index, tuple_desc, tuple)?;
        let data_start_off = slot.offset();
        if value.len() <= capacity {
            let new_slot = Slot::new(slot.offset() as u32, value.len() as u32);
            let slot_binary = new_slot.to_binary_buf();
            let up_slot = UpdateDelta::new(
                field.slot().offset() as u32,
                Slot::size_of() as u32,
                slot_binary,
            );
            let up_data =
                UpdateDelta::new(slot.offset() as u32, slot.length() as u32, value.clone());
            delta.push(up_slot);
            delta.push(up_data);
        } else {
            let slot_start_off = field.slot().offset();
            let total_field_num = tuple_desc.field_count();
            let mut _offset = data_start_off;
            let mut _written_field_num = total_field_num - index;

            let mut buf_slot = Buf::new();
            let mut buf_data = Buf::new();
            buf_slot.resize(Slot::size_of() * (total_field_num - index), 0);
            {
                // write updated field
                let mut new_data = value.clone();
                let new_slot = Slot::new(slot.offset() as u32, new_data.len() as u32);
                new_slot.to_binary(&mut buf_slot[_written_field_num * Slot::size_of()..]);
                buf_data.append(&mut new_data);
                _offset += buf_data.len();
                _written_field_num += 1;
            }
            while _written_field_num < total_field_num - index {
                let i = _written_field_num + index;
                let field = tuple_desc.get_field_desc(i);
                let binary = read_binary_data(field, tuple)?;
                let new_slot = Slot::new(_offset as u32, binary.len() as u32);
                new_slot.to_binary(&mut buf_slot[_written_field_num * Slot::size_of()..]);
                buf_data.extend_from_slice(binary);
                _offset += slot.length();
                _written_field_num += 1;
            }

            let up_slot = UpdateDelta::new(
                slot_start_off as u32,
                (Slot::size_of() * _written_field_num) as u32,
                buf_slot,
            );
            let up_data = UpdateDelta::new(
                data_start_off as u32,
                (tuple.len() - data_start_off) as u32,
                buf_data,
            );
            delta.push(up_slot);
            delta.push(up_data);
        }
    };
    Ok(())
}
