//! Reading of fixed- and variable-length values from a binary tuple.

use crate::tuple::field_desc::FieldDesc;
use crate::tuple::slot::Slot;
use crate::tuple::tuple_binary::TupleSlice;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

/// Reads and bounds-checks the slot of a variable-length field.
pub fn read_slot(field_desc: &FieldDesc, tuple: &TupleSlice) -> RS<Slot> {
    let _slot = field_desc.slot();
    if _slot.offset() + Slot::size_of() > tuple.len() {
        return Err(mudu_error!(ErrorCode::IndexOutOfRange));
    };
    let slot = Slot::from_binary(&tuple[_slot.offset().._slot.offset() + Slot::size_of()])?;
    if slot.offset() + slot.length() > tuple.len() {
        return Err(mudu_error!(ErrorCode::IndexOutOfRange));
    }
    Ok(slot)
}

/// Reads a fixed-length value at `offset` of `size` bytes from `tuple`.
pub fn read_fixed_len_value(offset: usize, size: usize, tuple: &TupleSlice) -> RS<&[u8]> {
    let _offset = offset;
    let _size = size;
    if tuple.len() < _offset + _size {
        return Err(mudu_error!(ErrorCode::IndexOutOfRange));
    }

    Ok(&tuple[_offset..(_offset + _size)])
}

/// Reads a variable-length value whose slot sits at `offset` in `tuple`.
pub fn read_var_len_value(offset: usize, tuple: &TupleSlice) -> RS<&[u8]> {
    let _offset = offset;
    if tuple.len() < _offset + Slot::size_of() {
        Err(mudu_error!(ErrorCode::IndexOutOfRange))
    } else {
        let slot = Slot::from_binary(&tuple[_offset.._offset + Slot::size_of()])?;
        if tuple.len() < slot.offset() + slot.length() {
            return Err(mudu_error!(ErrorCode::IndexOutOfRange));
        }
        Ok(&tuple[slot.offset()..slot.offset() + slot.length()])
    }
}

/// Reads the binary value of the field described by `desc` from `tuple`.
pub fn read_binary_data<'a>(desc: &FieldDesc, tuple: &'a TupleSlice) -> RS<&'a [u8]> {
    if desc.is_fixed_len() {
        read_fixed_len_value(desc.slot().offset(), desc.slot().length(), tuple)
    } else {
        read_var_len_value(desc.slot().offset(), tuple)
    }
}
