#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::tuple::field_desc::FieldDesc;
    use crate::tuple::slot::Slot;
    use mudu::common::serde_utils::{deserialize_from_json, serialize_to_json};
    use mudu::error::ErrorCode;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;

    fn i32_field() -> FieldDesc {
        FieldDesc::new(Slot::new(0, 4), DatType::new_no_param(DatTypeID::I32), true)
    }

    fn string_field() -> FieldDesc {
        FieldDesc::new_with_nullability(
            Slot::new(0, 8),
            DatType::default_for(DatTypeID::String),
            false,
            true,
            Some(0),
        )
    }

    #[test]
    fn field_desc_new_and_accessors() {
        let desc = i32_field();
        assert_eq!(desc.id(), 0);
        assert_eq!(desc.data_type(), DatTypeID::I32);
        assert_eq!(desc.type_obj().dat_type_id(), DatTypeID::I32);
        assert!(desc.is_fixed_len());
        assert!(!desc.nullable());
        assert_eq!(desc.null_bit_idx(), None);
    }

    #[test]
    fn field_desc_nullable_accessors() {
        let desc = string_field();
        assert!(!desc.is_fixed_len());
        assert!(desc.nullable());
        assert_eq!(desc.null_bit_idx(), Some(0));
    }

    #[test]
    fn field_desc_get_fixed_len_value() {
        let desc = i32_field();
        let tuple = 42i32.to_be_bytes();
        let value = desc.get(&tuple).unwrap();
        assert_eq!(value, &tuple);
    }

    #[test]
    fn field_desc_get_var_len_value() {
        let desc = string_field();
        // Build a tuple where the first 8 bytes are a slot pointing to the payload.
        let slot = Slot::new(8, 5);
        let mut tuple = slot.to_binary_buf().unwrap();
        tuple.extend_from_slice(b"hello");
        let value = desc.get(&tuple).unwrap();
        assert_eq!(value, b"hello");
    }

    #[test]
    fn field_desc_get_rejects_truncated_tuple() {
        let desc = i32_field();
        let err = desc.get(&[0, 0]).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
    }

    #[test]
    fn field_desc_json_round_trip() {
        let desc = string_field();
        let json = serialize_to_json(&desc).unwrap();
        let restored: FieldDesc = deserialize_from_json(&json).unwrap();
        assert_eq!(restored.data_type(), desc.data_type());
        assert_eq!(restored.is_fixed_len(), desc.is_fixed_len());
        assert_eq!(restored.nullable(), desc.nullable());
        assert_eq!(restored.null_bit_idx(), desc.null_bit_idx());
    }
}
