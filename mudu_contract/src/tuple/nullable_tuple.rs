//! `tuple::nullable_tuple` module.
#![allow(missing_docs)]

use crate::tuple::bitmap::Bitmap;
use crate::tuple::slot::Slot;
use crate::tuple::tuple_binary::TupleBinary;
use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_type::dat_value::DatValue;

#[derive(Clone, Debug)]
pub enum NullableValue {
    Null,
    Value(DatValue),
}

pub struct TupleBuilder<'a> {
    desc: &'a TupleBinaryDesc,
}

impl<'a> TupleBuilder<'a> {
    pub fn new(desc: &'a TupleBinaryDesc) -> Self {
        Self { desc }
    }

    pub fn build(&self, values: &[NullableValue]) -> RS<TupleBinary> {
        if values.len() != self.desc.field_count() {
            return Err(mudu_error!(
                ErrorCode::Parse,
                format!(
                    "value length {} does not match tuple field count {}",
                    values.len(),
                    self.desc.field_count()
                )
            ));
        }

        let mut bitmap = Bitmap::new(self.desc.nullable_count());
        let mut tuple = vec![0; self.desc.min_tuple_size()];
        let mut var_data_offset = self.desc.min_tuple_size();

        for (index, (field, value)) in self.desc.field_desc().iter().zip(values.iter()).enumerate()
        {
            match value {
                NullableValue::Null => {
                    let Some(bit_idx) = field.null_bit_idx() else {
                        return Err(mudu_error!(
                            ErrorCode::InvalidTuple,
                            format!("cannot write NULL into NOT NULL field {}", index)
                        ));
                    };
                    bitmap.set(bit_idx as usize, true)?;
                }
                NullableValue::Value(value) => {
                    if let Some(bit_idx) = field.null_bit_idx() {
                        bitmap.set(bit_idx as usize, false)?;
                    }
                    let type_obj = field.type_obj();
                    let binary: Vec<u8> = type_obj.dat_type_id().fn_send()(value, type_obj)
                        .map_err(|e| {
                            mudu_error!(ErrorCode::InvalidType, format!("fn_send failed: {e}"))
                        })?
                        .into();
                    if field.is_fixed_len() {
                        let offset = field.slot().offset();
                        let len = field.slot().length();
                        tuple[offset..offset + len].copy_from_slice(&binary);
                    } else {
                        let offset = var_data_offset;
                        tuple.extend_from_slice(&binary);
                        var_data_offset += binary.len();
                        let slot_offset = field.slot().offset();
                        Slot::new(offset as u32, binary.len() as u32)
                            .to_binary(&mut tuple[slot_offset..slot_offset + Slot::size_of()])?;
                    }
                }
            }
        }

        tuple[..self.desc.null_bitmap_size()].copy_from_slice(bitmap.as_bytes());
        Ok(tuple)
    }
}

pub fn read_value(
    tuple: &TupleBinary,
    schema: &TupleBinaryDesc,
    col_idx: usize,
) -> RS<NullableValue> {
    if col_idx >= schema.field_count() {
        return Err(mudu_error!(
            ErrorCode::IndexOutOfRange,
            format!("field index {} out of {}", col_idx, schema.field_count())
        ));
    }
    let field = schema.get_field_desc(col_idx);
    if tuple.len() < schema.min_tuple_size() {
        return Err(mudu_error!(
            ErrorCode::Decode,
            format!(
                "tuple len {} is less than minimum tuple size {}",
                tuple.len(),
                schema.min_tuple_size()
            )
        ));
    }

    if let Some(bit_idx) = field.null_bit_idx() {
        let bitmap =
            Bitmap::from_bytes(schema.nullable_count(), &tuple[..schema.null_bitmap_size()])?;
        if bitmap.get(bit_idx as usize)? {
            return Ok(NullableValue::Null);
        }
    }

    let bytes = if field.is_fixed_len() {
        let offset = field.slot().offset();
        let len = field.slot().length();
        &tuple[offset..offset + len]
    } else {
        let slot_offset = field.slot().offset();
        let slot = Slot::from_binary(&tuple[slot_offset..slot_offset + Slot::size_of()])?;
        if slot.offset() + slot.length() > tuple.len() {
            return Err(mudu_error!(ErrorCode::IndexOutOfRange));
        }
        &tuple[slot.offset()..slot.offset() + slot.length()]
    };

    let type_obj = field.type_obj();
    let (value, _) = type_obj.dat_type_id().fn_recv()(bytes, type_obj).map_err(|e| {
        mudu_error!(
            ErrorCode::TypeConversionFailed,
            "convert binary to value error",
            e
        )
    })?;
    Ok(NullableValue::Value(value))
}
