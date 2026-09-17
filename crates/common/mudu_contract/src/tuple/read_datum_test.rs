#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::tuple::build_tuple::build_tuple;
    use crate::tuple::field_desc::FieldDesc;
    use crate::tuple::read_datum::{
        read_binary_data, read_fixed_len_value, read_slot, read_var_len_value,
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
