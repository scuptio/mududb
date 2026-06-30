#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::tuple::field_desc::FieldDesc;
    use crate::tuple::slot::Slot;
    use crate::tuple::write_value::{
        write_slot_to_buf, write_slot_to_tuple, write_value_to_buf, write_value_to_tuple,
        write_value_to_tuple_with_max_size_opt,
    };
    use mudu::common::buf::Buf;
    use mudu::error::ErrorCode;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;

    fn i32_field_desc() -> FieldDesc {
        FieldDesc::new(Slot::new(0, 4), DatType::new_no_param(DatTypeID::I32), true)
    }

    fn string_field_desc() -> FieldDesc {
        FieldDesc::new(
            Slot::new(0, 8),
            DatType::default_for(DatTypeID::String),
            false,
        )
    }

    #[test]
    fn write_slot_to_buf_roundtrips_slot() {
        let mut buf = vec![0u8; Slot::size_of()];
        write_slot_to_buf(42, 100, &mut buf).unwrap();
        let slot = Slot::from_binary(&buf).unwrap();
        assert_eq!(slot.offset(), 42);
        assert_eq!(slot.length(), 100);
    }

    #[test]
    fn write_slot_to_buf_rejects_small_buffer() {
        let mut buf = vec![0u8; Slot::size_of() - 1];
        let err = write_slot_to_buf(0, 0, &mut buf).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    }

    #[test]
    fn write_slot_to_tuple_skips_fixed_len_field() {
        let field = i32_field_desc();
        let mut tuple = vec![0u8; 8];
        write_slot_to_tuple(&field, 10, 4, &mut tuple).unwrap();
        // No slot should have been written; tuple remains zeroed.
        assert!(tuple.iter().all(|&b| b == 0));
    }

    #[test]
    fn write_slot_to_tuple_writes_var_len_slot() {
        let field = string_field_desc();
        let mut tuple = vec![0u8; 16];
        write_slot_to_tuple(&field, 8, 5, &mut tuple).unwrap();
        let slot = Slot::from_binary(&tuple[0..Slot::size_of()]).unwrap();
        assert_eq!(slot.offset(), 8);
        assert_eq!(slot.length(), 5);
    }

    #[test]
    fn write_slot_to_tuple_rejects_slot_out_of_bounds() {
        let field = FieldDesc::new(
            Slot::new(20, 8),
            DatType::default_for(DatTypeID::String),
            false,
        );
        let mut tuple = vec![0u8; 16];
        let err = write_slot_to_tuple(&field, 0, 4, &mut tuple).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
    }

    #[test]
    fn write_value_to_buf_accepts_fitting_value() {
        let field = i32_field_desc();
        let value: Buf = vec![1, 2, 3, 4];
        let mut buf = vec![0u8; 4];
        let result = write_value_to_buf(&field, &value, &mut buf).unwrap();
        assert_eq!(result, Ok(4));
        assert_eq!(buf, value);
    }

    #[test]
    fn write_value_to_buf_returns_required_size_for_overflow() {
        let field = i32_field_desc();
        let value: Buf = vec![1, 2, 3, 4, 5];
        let mut buf = vec![0u8; 4];
        let result = write_value_to_buf(&field, &value, &mut buf).unwrap();
        assert_eq!(result, Err(5));
        assert!(buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn write_value_to_tuple_writes_fixed_value() {
        let field = i32_field_desc();
        let value: Buf = vec![1, 2, 3, 4];
        let mut tuple = vec![0u8; 8];
        let result = write_value_to_tuple(&field, 0, &value, &mut tuple).unwrap();
        assert_eq!(result, Ok(4));
        assert_eq!(&tuple[0..4], value.as_slice());
    }

    #[test]
    fn write_value_to_tuple_with_max_size_opt_limits_slice() {
        let field = string_field_desc();
        let value: Buf = vec![b'h', b'e', b'l', b'l', b'o'];
        let mut tuple = vec![0u8; 32];
        let result =
            write_value_to_tuple_with_max_size_opt(&field, 8, Some(8), &value, &mut tuple).unwrap();
        assert_eq!(result, Ok(5));
        assert_eq!(&tuple[8..13], value.as_slice());
    }

    #[test]
    fn write_value_to_tuple_with_max_size_opt_none_uses_tail() {
        let field = string_field_desc();
        let value: Buf = vec![b'w', b'o', b'r', b'l', b'd'];
        let mut tuple = vec![0u8; 32];
        let result =
            write_value_to_tuple_with_max_size_opt(&field, 4, None, &value, &mut tuple).unwrap();
        assert_eq!(result, Ok(5));
        assert_eq!(&tuple[4..9], value.as_slice());
    }
}
