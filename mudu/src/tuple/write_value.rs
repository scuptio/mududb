use crate::common::buf::Buf;
use crate::common::result::RS;
use crate::error::ec::EC;
use crate::error::err::MError;
use crate::m_error;
use crate::tuple::field_desc::FieldDesc;
use crate::tuple::slot::Slot;
use crate::tuple::tuple_binary::TupleSlice;

pub fn write_slot_to_buf(value_offset: usize, value_size: usize, buf: &mut [u8]) -> RS<()> {
    let slot = Slot::new(value_offset as u32, value_size as u32);
    if Slot::size_of() < buf.len() {
        return Err(m_error!(EC::NotImplemented));
    }
    slot.to_binary(buf);
    Ok(())
}

pub fn write_slot_to_tuple(
    field: &FieldDesc,
    value_offset: usize,
    value_size: usize,
    tuple: &mut TupleSlice,
) -> RS<()> {
    if !field.is_fixed_len() {
        let slot_offset = field.slot().offset();
        if slot_offset + Slot::size_of() > tuple.len() {
            panic!("Slot offset out of bounds");
        }
        write_slot_to_buf(
            value_offset,
            value_size,
            &mut tuple[slot_offset..slot_offset + Slot::size_of()],
        )?;
    }
    Ok(())
}

pub fn write_value_to_buf(
    _desc: &FieldDesc,
    value: &Buf,
    buf: &mut [u8],
) -> RS<Result<usize, usize>> {
    let r = {
        if value.len() > buf.len() {
            return Err(m_error!(EC::InternalErr, "buffer size error "));
        }
        buf[0..value.len()].copy_from_slice(value);
        Ok::<_, MError>(value.len())
    };

    let len = match r {
        Ok(n) => n,
        Err(e) => return Err(e),
    };
    Ok(Ok(len))
}

pub fn write_value_to_tuple(
    desc: &FieldDesc,
    value_offset: usize,
    value: &Buf,
    tuple: &mut TupleSlice,
) -> RS<Result<usize, usize>> {
    write_value_to_tuple_with_max_size_opt(desc, value_offset, None, value, tuple)
}

pub fn write_value_to_tuple_with_max_size_opt(
    desc: &FieldDesc,
    value_offset: usize,
    value_opt_max_size: Option<usize>,
    value: &Buf,
    tuple: &mut TupleSlice,
) -> RS<Result<usize, usize>> {
    let buf = match value_opt_max_size {
        Some(max_size) => &mut tuple[value_offset..value_offset + max_size],
        None => &mut tuple[value_offset..],
    };
    write_value_to_buf(desc, value, buf)
}
