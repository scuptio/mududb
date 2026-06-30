//! `tuple::read_datum` module.
#![allow(missing_docs)]

use crate::tuple::field_desc::FieldDesc;
use crate::tuple::slot::Slot;
use crate::tuple::tuple_binary::TupleSlice;
use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

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

pub fn read_data_capacity(
    index: usize,
    tuple_desc: &TupleBinaryDesc,
    tuple: &TupleSlice,
) -> RS<usize> {
    if index >= tuple_desc.field_count() {
        return Err(mudu_error!(ErrorCode::IndexOutOfRange));
    }
    let field = tuple_desc.get_field_desc(index);
    if field.is_fixed_len() {
        Ok(field.slot().length())
    } else {
        let slot = read_slot(field, tuple)?;
        if index + 1 == tuple_desc.field_count() {
            Ok(tuple.len() - field.slot().offset())
        } else {
            let field_next = tuple_desc.get_field_desc(index + 1);
            assert!(!field_next.is_fixed_len());
            let slot_next = read_slot(field_next, tuple)?;
            if slot.offset() > slot_next.offset()
                || slot_next.offset() + slot_next.length() > tuple.len()
            {
                return Err(mudu_error!(ErrorCode::InvalidTuple));
            }
            let size = slot_next.offset() - slot.offset();
            if size < slot.length() {
                return Err(mudu_error!(ErrorCode::InvalidTuple));
            }
            Ok(size)
        }
    }
}

pub fn read_fixed_len_value(offset: usize, size: usize, tuple: &TupleSlice) -> RS<&[u8]> {
    let _offset = offset;
    let _size = size;
    if tuple.len() < _offset + _size {
        return Err(mudu_error!(ErrorCode::IndexOutOfRange));
    }

    Ok(&tuple[_offset..(_offset + _size)])
}

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

pub fn read_binary_data<'a>(desc: &FieldDesc, tuple: &'a TupleSlice) -> RS<&'a [u8]> {
    if desc.is_fixed_len() {
        read_fixed_len_value(desc.slot().offset(), desc.slot().length(), tuple)
    } else {
        read_var_len_value(desc.slot().offset(), tuple)
    }
}
