use crate::common::error::ER;
use crate::common::result::RS;
use crate::data_type::dt_fn_base::ErrConvert;
use crate::tuple::datum::Datum;
use crate::tuple::field_desc::FieldDesc;
use crate::tuple::slot::Slot;
use crate::tuple::tuple_raw::TupleRaw;

pub fn write_slot_to_buf(value_offset: usize, value_size: usize, buf: &mut [u8]) -> RS<()> {
    let slot = Slot::new(value_offset as u32, value_size as u32);
    if Slot::size_of() < buf.len() {
        return Err(ER::NotImplemented);
    }
    slot.to_binary(buf);
    Ok(())
}

pub fn write_slot_to_tuple(
    field: &FieldDesc,
    value_offset: usize,
    value_size: usize,
    tuple: &mut TupleRaw,
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
    desc: &FieldDesc,
    value: &Datum,
    buf: &mut [u8],
) -> RS<Result<usize, usize>> {
    let data_type = desc.data_type();
    let send_to = data_type.fn_send_to();
    let r = match value {
        Datum::Internal(d) => send_to(d, desc.type_param(), buf),
        Datum::Typed(d) => {
            let from_typed = data_type.fn_from_typed();
            let i =
                from_typed(d, desc.type_param()).map_err(|e| ER::InternalError(e.to_string()))?;
            send_to(&i, desc.type_param(), buf)
        }
        Datum::Printable(d) => {
            let input = data_type.fn_input();
            let i = input(d, desc.type_param()).map_err(|e| ER::InternalError(e.to_string()))?;
            send_to(&i, desc.type_param(), buf)
        }
        Datum::Binary(d) => {
            if d.buf().len() > buf.len() {
                return Err(ER::InternalError("buffer size error ".to_string()));
            }
            buf[0..d.buf().len()].copy_from_slice(d.buf());
            Ok(d.buf().len())
        }
        Datum::Null => Ok(0),
    };

    let len = match r {
        Ok(n) => n,
        Err(e) => {
            return match e {
                ErrConvert::ErrLowBufSpace(usize) => Ok(Err(usize)),
                _ => Err(ER::InternalError(e.to_string())),
            };
        }
    };
    Ok(Ok(len))
}

pub fn write_value_to_tuple(
    desc: &FieldDesc,
    value_offset: usize,
    value: &Datum,
    tuple: &mut TupleRaw,
) -> RS<Result<usize, usize>> {
    write_value_to_tuple_with_max_size_opt(desc, value_offset, None, value, tuple)
}

pub fn write_value_to_tuple_with_max_size_opt(
    desc: &FieldDesc,
    value_offset: usize,
    value_opt_max_size: Option<usize>,
    value: &Datum,
    tuple: &mut TupleRaw,
) -> RS<Result<usize, usize>> {
    let buf = match value_opt_max_size {
        Some(max_size) => &mut tuple[value_offset..value_offset + max_size],
        None => &mut tuple[value_offset..],
    };
    write_value_to_buf(desc, value, buf)
}
