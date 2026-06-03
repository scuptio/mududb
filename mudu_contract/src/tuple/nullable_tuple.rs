use crate::tuple::bitmap::Bitmap;
use crate::tuple::slot::Slot;
use crate::tuple::tuple_binary::TupleBinary;
use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
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
            return Err(m_error!(
                EC::ParseErr,
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
                        return Err(m_error!(
                            EC::TupleErr,
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
                    let binary = type_obj.dat_type_id().fn_send()(value, type_obj)
                        .map_err(|e| m_error!(EC::TypeErr, "convert value to binary error", e))?
                        .into();
                    if field.is_fixed_len() {
                        let offset = field.slot().offset();
                        let len = field.slot().length();
                        if binary.len() != len {
                            return Err(m_error!(
                                EC::EncodeErr,
                                format!(
                                    "fixed field {} expected {} bytes, got {}",
                                    index,
                                    len,
                                    binary.len()
                                )
                            ));
                        }
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
        return Err(m_error!(
            EC::IndexOutOfRange,
            format!("field index {} out of {}", col_idx, schema.field_count())
        ));
    }
    let field = schema.get_field_desc(col_idx);
    if tuple.len() < schema.min_tuple_size() {
        return Err(m_error!(
            EC::DecodeErr,
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
        if offset + len > tuple.len() {
            return Err(m_error!(EC::IndexOutOfRange));
        }
        &tuple[offset..offset + len]
    } else {
        let slot_offset = field.slot().offset();
        if slot_offset + Slot::size_of() > tuple.len() {
            return Err(m_error!(EC::IndexOutOfRange));
        }
        let slot = Slot::from_binary(&tuple[slot_offset..slot_offset + Slot::size_of()])?;
        if slot.offset() + slot.length() > tuple.len() {
            return Err(m_error!(EC::IndexOutOfRange));
        }
        &tuple[slot.offset()..slot.offset() + slot.length()]
    };

    let type_obj = field.type_obj();
    let (value, _) = type_obj.dat_type_id().fn_recv()(bytes, type_obj)
        .map_err(|e| m_error!(EC::TypeErr, "convert binary to value error", e))?;
    Ok(NullableValue::Value(value))
}

#[cfg(test)]
mod tests {
    use super::{read_value, NullableValue, TupleBuilder};
    use crate::tuple::datum_desc::DatumDesc;
    use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
    use crate::tuple::tuple_field_desc::TupleFieldDesc;
    use mudu::error::ec::EC;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dat_value::DatValue;

    fn i32_type() -> DatType {
        DatType::new_no_param(DatTypeID::I32)
    }

    fn string_type() -> DatType {
        DatType::default_for(DatTypeID::String)
    }

    fn desc(fields: Vec<DatumDesc>) -> (TupleBinaryDesc, Vec<usize>) {
        TupleFieldDesc::new(fields).to_tuple_binary_desc().unwrap()
    }

    fn physical_index(mapping: &[usize], logical_index: usize) -> usize {
        mapping
            .iter()
            .position(|index| *index == logical_index)
            .unwrap()
    }

    #[test]
    fn schema_assigns_null_bits_only_to_nullable_columns() {
        let (desc, mapping) = desc(vec![
            DatumDesc::new("id".to_string(), i32_type()),
            DatumDesc::new_nullable("name".to_string(), string_type(), true),
            DatumDesc::new_nullable("age".to_string(), i32_type(), true),
        ]);
        assert_eq!(desc.nullable_count(), 2);
        assert_eq!(
            desc.get_field_desc(physical_index(&mapping, 0))
                .null_bit_idx(),
            None
        );
        assert_eq!(
            desc.get_field_desc(physical_index(&mapping, 1))
                .null_bit_idx(),
            Some(0)
        );
        assert_eq!(
            desc.get_field_desc(physical_index(&mapping, 2))
                .null_bit_idx(),
            Some(1)
        );
        assert_eq!(desc.null_bitmap_size(), 8);
    }

    #[test]
    fn builder_sets_null_and_non_null_bits() {
        let (desc, mapping) = desc(vec![
            DatumDesc::new("id".to_string(), i32_type()),
            DatumDesc::new_nullable("name".to_string(), string_type(), true),
            DatumDesc::new_nullable("age".to_string(), i32_type(), true),
        ]);
        let mut values = vec![NullableValue::Null; desc.field_count()];
        values[physical_index(&mapping, 0)] = NullableValue::Value(DatValue::from_i32(7));
        values[physical_index(&mapping, 1)] =
            NullableValue::Value(DatValue::from_string("alice".to_string()));
        values[physical_index(&mapping, 2)] = NullableValue::Null;
        let tuple = TupleBuilder::new(&desc).build(&values).unwrap();
        assert_eq!(tuple[0] & 0b0000_0001, 0);
        assert_ne!(tuple[0] & 0b0000_0010, 0);
    }

    #[test]
    fn builder_rejects_null_for_not_null_column() {
        let (desc, _) = desc(vec![DatumDesc::new("id".to_string(), i32_type())]);
        let err = TupleBuilder::new(&desc)
            .build(&[NullableValue::Null])
            .unwrap_err();
        assert_eq!(err.ec(), EC::TupleErr);
    }

    #[test]
    fn read_null_does_not_decode_payload() {
        let (desc, _) = desc(vec![DatumDesc::new_nullable(
            "name".to_string(),
            string_type(),
            true,
        )]);
        let tuple = TupleBuilder::new(&desc)
            .build(&[NullableValue::Null])
            .unwrap();
        assert_eq!(tuple.len(), desc.min_tuple_size());
        match read_value(&tuple, &desc, 0).unwrap() {
            NullableValue::Null => {}
            NullableValue::Value(_) => panic!("expected NULL"),
        }
    }

    #[test]
    fn varlen_null_does_not_write_payload_but_non_null_does() {
        let (desc, _) = desc(vec![DatumDesc::new_nullable(
            "name".to_string(),
            string_type(),
            true,
        )]);
        let null_tuple = TupleBuilder::new(&desc)
            .build(&[NullableValue::Null])
            .unwrap();
        let value_tuple = TupleBuilder::new(&desc)
            .build(&[NullableValue::Value(DatValue::from_string(
                "bob".to_string(),
            ))])
            .unwrap();
        assert_eq!(null_tuple.len(), desc.min_tuple_size());
        assert!(value_tuple.len() > desc.min_tuple_size());
    }

    #[test]
    fn read_value_roundtrips_fixed_and_varlen_values() {
        let (desc, mapping) = desc(vec![
            DatumDesc::new("id".to_string(), i32_type()),
            DatumDesc::new_nullable("name".to_string(), string_type(), true),
        ]);
        let id_idx = physical_index(&mapping, 0);
        let name_idx = physical_index(&mapping, 1);
        let mut values = vec![NullableValue::Null; desc.field_count()];
        values[id_idx] = NullableValue::Value(DatValue::from_i32(11));
        values[name_idx] = NullableValue::Value(DatValue::from_string("carol".to_string()));
        let tuple = TupleBuilder::new(&desc).build(&values).unwrap();
        match read_value(&tuple, &desc, id_idx).unwrap() {
            NullableValue::Value(value) => assert_eq!(*value.expect_i32(), 11),
            NullableValue::Null => panic!("expected value"),
        }
        match read_value(&tuple, &desc, name_idx).unwrap() {
            NullableValue::Value(value) => assert_eq!(value.expect_string(), "carol"),
            NullableValue::Null => panic!("expected value"),
        }
    }
}
