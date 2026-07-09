#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::tuple::build_tuple::build_tuple;
    use crate::tuple::field_desc::FieldDesc;
    use crate::tuple::read_datum::{
        read_binary_data, read_data_capacity, read_fixed_len_value, read_slot, read_var_len_value,
    };
    use crate::tuple::slot::Slot;
    use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
    use mudu::error::ErrorCode;
    use mudu_type::data_type::DataType;
    use mudu_type::type_family::TypeFamily;

    fn i32_desc() -> TupleBinaryDesc {
        TupleBinaryDesc::from(vec![DataType::new_no_param(TypeFamily::I32)]).unwrap()
    }

    fn string_desc() -> TupleBinaryDesc {
        TupleBinaryDesc::from(vec![DataType::default_for(TypeFamily::String)]).unwrap()
    }

    #[test]
    fn read_slot_decodes_valid_slot() {
        let desc = string_desc();
        let tuple = build_tuple(&[b"hello".to_vec()], &desc).unwrap();
        let field = desc.get_field_desc(0);
        let slot = read_slot(field, &tuple).unwrap();
        assert_eq!(slot.length(), 5);
    }

    #[test]
    fn read_slot_rejects_slot_out_of_tuple() {
        let field = FieldDesc::new(
            Slot::new(20, 8),
            DataType::default_for(TypeFamily::String),
            false,
        );
        let tuple = vec![0u8; 8];
        let err = read_slot(&field, &tuple).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
    }

    #[test]
    fn read_slot_rejects_value_out_of_tuple() {
        // Slot decodes successfully but points past the tuple.
        let mut tuple = vec![0u8; 16];
        Slot::new(100, 5)
            .to_binary(&mut tuple[0..Slot::size_of()])
            .unwrap();
        let field = FieldDesc::new(
            Slot::new(0, 8),
            DataType::default_for(TypeFamily::String),
            false,
        );
        let err = read_slot(&field, &tuple).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
    }

    #[test]
    fn read_data_capacity_for_fixed_len_field() {
        let desc = i32_desc();
        let tuple = build_tuple(&[42i32.to_le_bytes().to_vec()], &desc).unwrap();
        let capacity = read_data_capacity(0, &desc, &tuple).unwrap();
        assert_eq!(capacity, 4);
    }

    #[test]
    fn read_data_capacity_rejects_out_of_range_index() {
        let desc = i32_desc();
        let tuple = build_tuple(&[42i32.to_le_bytes().to_vec()], &desc).unwrap();
        let err = read_data_capacity(5, &desc, &tuple).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
    }

    #[test]
    fn read_data_capacity_for_last_var_len_field() {
        let desc = string_desc();
        let tuple = build_tuple(&[b"hello".to_vec()], &desc).unwrap();
        let capacity = read_data_capacity(0, &desc, &tuple).unwrap();
        let field = desc.get_field_desc(0);
        // Capacity for the last variable-length field spans from its slot offset to the tuple end.
        assert_eq!(capacity, tuple.len() - field.slot().offset());
        assert!(capacity >= 5);
    }

    #[test]
    fn read_data_capacity_for_non_last_var_len_field() {
        let desc = TupleBinaryDesc::from(vec![
            DataType::default_for(TypeFamily::String),
            DataType::default_for(TypeFamily::String),
        ])
        .unwrap();
        let tuple = build_tuple(&[b"hi".to_vec(), b"world".to_vec()], &desc).unwrap();
        let capacity = read_data_capacity(0, &desc, &tuple).unwrap();
        let first_slot = read_slot(desc.get_field_desc(0), &tuple).unwrap();
        let second_slot = read_slot(desc.get_field_desc(1), &tuple).unwrap();
        assert_eq!(capacity, second_slot.offset() - first_slot.offset());
    }

    #[test]
    fn read_data_capacity_rejects_value_extending_past_tuple() {
        let desc = string_desc();
        let mut tuple = build_tuple(&[b"hello".to_vec()], &desc).unwrap();
        // Corrupt the slot so the data region extends past the tuple.
        let valid_offset = Slot::from_binary(&tuple[0..Slot::size_of()])
            .unwrap()
            .offset() as u32;
        Slot::new(valid_offset, 200)
            .to_binary(&mut tuple[0..Slot::size_of()])
            .unwrap();
        let err = read_data_capacity(0, &desc, &tuple).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
    }

    #[test]
    fn read_data_capacity_rejects_invalid_next_slot() {
        let desc = TupleBinaryDesc::from(vec![
            DataType::default_for(TypeFamily::String),
            DataType::default_for(TypeFamily::String),
        ])
        .unwrap();
        let mut tuple = build_tuple(&[b"hi".to_vec(), b"world".to_vec()], &desc).unwrap();
        // Corrupt the second slot so it points before the first data region.
        let first_slot_offset = desc.get_field_desc(0).slot().offset();
        let second_slot_offset = desc.get_field_desc(1).slot().offset();
        let first_data_offset =
            Slot::from_binary(&tuple[first_slot_offset..first_slot_offset + Slot::size_of()])
                .unwrap()
                .offset();
        let second_slot_range = second_slot_offset..second_slot_offset + Slot::size_of();
        Slot::new(first_data_offset as u32 - 1, 1)
            .to_binary(&mut tuple[second_slot_range])
            .unwrap();
        let err = read_data_capacity(0, &desc, &tuple).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidTuple);
    }

    #[test]
    fn read_data_capacity_rejects_too_small_gap_between_slots() {
        let desc = TupleBinaryDesc::from(vec![
            DataType::default_for(TypeFamily::String),
            DataType::default_for(TypeFamily::String),
        ])
        .unwrap();
        let mut tuple = build_tuple(&[b"hi".to_vec(), b"world".to_vec()], &desc).unwrap();
        // Move the second data slot so the gap is smaller than the first field's length.
        let first_slot_offset = desc.get_field_desc(0).slot().offset();
        let second_slot_offset = desc.get_field_desc(1).slot().offset();
        let first_slot =
            Slot::from_binary(&tuple[first_slot_offset..first_slot_offset + Slot::size_of()])
                .unwrap();
        let second_slot_range = second_slot_offset..second_slot_offset + Slot::size_of();
        Slot::new(first_slot.offset() as u32 + 1, 1)
            .to_binary(&mut tuple[second_slot_range])
            .unwrap();
        let err = read_data_capacity(0, &desc, &tuple).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidTuple);
    }

    #[test]
    fn read_fixed_len_value_reads_data() {
        let tuple = vec![0, 0, 0, 42];
        let data = read_fixed_len_value(0, 4, &tuple).unwrap();
        assert_eq!(data, &[0, 0, 0, 42]);
    }

    #[test]
    fn read_fixed_len_value_rejects_short_tuple() {
        let tuple = vec![0, 0, 0];
        let err = read_fixed_len_value(0, 4, &tuple).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
    }

    #[test]
    fn read_var_len_value_reads_data() {
        let mut tuple = vec![0u8; 16];
        Slot::new(8, 5)
            .to_binary(&mut tuple[0..Slot::size_of()])
            .unwrap();
        tuple[8..13].copy_from_slice(b"hello");
        let data = read_var_len_value(0, &tuple).unwrap();
        assert_eq!(data, b"hello");
    }

    #[test]
    fn read_var_len_value_rejects_slot_out_of_tuple() {
        let tuple = vec![0u8; 4];
        let err = read_var_len_value(0, &tuple).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
    }

    #[test]
    fn read_var_len_value_rejects_value_out_of_tuple() {
        let mut tuple = vec![0u8; 16];
        Slot::new(8, 20)
            .to_binary(&mut tuple[0..Slot::size_of()])
            .unwrap();
        let err = read_var_len_value(0, &tuple).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
    }

    #[test]
    fn read_binary_data_fixed_len() {
        let desc = i32_desc();
        let tuple = build_tuple(&[42i32.to_le_bytes().to_vec()], &desc).unwrap();
        let field = desc.get_field_desc(0);
        let data = read_binary_data(field, &tuple).unwrap();
        assert_eq!(data, 42i32.to_le_bytes());
    }

    #[test]
    fn read_binary_data_var_len() {
        let desc = string_desc();
        let tuple = build_tuple(&[b"hello".to_vec()], &desc).unwrap();
        let field = desc.get_field_desc(0);
        let data = read_binary_data(field, &tuple).unwrap();
        assert_eq!(data, b"hello");
    }
}
