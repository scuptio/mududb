#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::tuple::datum_desc::DatumDesc;
    use crate::tuple::tuple_field_desc::TupleFieldDesc;
    use mudu::common::serde_utils::{deserialize_from_json, serialize_to_json};
    use mudu::error::ErrorCode;
    use mudu_type::data_type::DataType;
    use mudu_type::type_family::TypeFamily;

    fn i32_field(name: &str) -> DatumDesc {
        DatumDesc::new(name.to_string(), DataType::new_no_param(TypeFamily::I32))
    }

    fn nullable_i32_field(name: &str) -> DatumDesc {
        DatumDesc::new_nullable(
            name.to_string(),
            DataType::new_no_param(TypeFamily::I32),
            true,
        )
    }

    #[test]
    fn tuple_field_desc_new_and_accessors() {
        let fields = vec![i32_field("a"), i32_field("b")];
        let desc = TupleFieldDesc::new(fields.clone());
        assert_eq!(desc.fields().len(), 2);
        assert_eq!(desc.fields()[0].name(), "a");
        assert_eq!(desc.fields()[1].name(), "b");
    }

    #[test]
    fn tuple_field_desc_into_fields() {
        let fields = vec![i32_field("a")];
        let desc = TupleFieldDesc::new(fields);
        let extracted = desc.into_fields();
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0].name(), "a");
    }

    #[test]
    fn tuple_field_desc_into_returns_datum_desc_vec() {
        let fields = vec![i32_field("a")];
        let desc = TupleFieldDesc::new(fields);
        let extracted: Vec<DatumDesc> = desc.into();
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0].name(), "a");
    }

    #[test]
    fn tuple_field_desc_as_ref_returns_self() {
        let desc = TupleFieldDesc::new(vec![i32_field("a")]);
        let reference: &TupleFieldDesc = desc.as_ref();
        assert_eq!(reference.fields().len(), 1);
    }

    #[test]
    fn tuple_field_desc_to_tuple_binary_desc() {
        let desc = TupleFieldDesc::new(vec![i32_field("a"), i32_field("b")]);
        let (binary_desc, mapping) = desc.to_tuple_binary_desc().unwrap();
        assert_eq!(binary_desc.field_count(), 2);
        assert_eq!(mapping.len(), 2);
    }

    #[test]
    fn tuple_field_desc_to_tuple_binary_desc_with_nullable() {
        let desc = TupleFieldDesc::new(vec![
            nullable_i32_field("a"),
            i32_field("b"),
            nullable_i32_field("c"),
        ]);
        let (binary_desc, mapping) = desc.to_tuple_binary_desc().unwrap();
        assert_eq!(binary_desc.field_count(), 3);
        assert_eq!(binary_desc.nullable_count(), 2);
        assert_eq!(mapping.len(), 3);
    }

    #[test]
    fn tuple_field_desc_serialize_deserialize_round_trip() {
        let fields = vec![
            i32_field("c1"),
            DatumDesc::new("c2".to_string(), DataType::new_no_param(TypeFamily::I64)),
            DatumDesc::new("c3".to_string(), DataType::new_no_param(TypeFamily::I32)),
        ];

        let original_desc = TupleFieldDesc::new(fields);
        let bytes = original_desc.serialize_to().unwrap();
        let restored_desc = TupleFieldDesc::deserialize_from(&bytes).unwrap();
        assert_eq!(original_desc.fields().len(), restored_desc.fields().len());
        for (original, restored) in original_desc
            .fields()
            .iter()
            .zip(restored_desc.fields().iter())
        {
            assert_eq!(original.name(), restored.name());
            assert_eq!(original.type_family(), restored.type_family());
            assert_eq!(original.nullable(), restored.nullable());
        }
    }

    #[test]
    fn tuple_field_desc_json_round_trip() {
        let fields = vec![i32_field("a"), nullable_i32_field("b")];
        let original_desc = TupleFieldDesc::new(fields);
        let json = serialize_to_json(&original_desc).unwrap();
        let restored_desc: TupleFieldDesc = deserialize_from_json(&json).unwrap();
        assert_eq!(original_desc.fields().len(), restored_desc.fields().len());
    }

    #[test]
    fn tuple_field_desc_deserialize_rejects_invalid_bytes() {
        let err = TupleFieldDesc::deserialize_from(&[0, 1, 2, 3, 4]).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InsufficientBufferSpace);
    }

    #[test]
    fn tuple_field_desc_to_record_type() {
        let desc = TupleFieldDesc::new(vec![i32_field("a"), i32_field("b")]);
        let record_type = desc.to_record_type("my_record".to_string()).unwrap();
        assert_eq!(record_type.type_family(), TypeFamily::Record);
        let param = record_type.as_record_param().unwrap();
        assert_eq!(param.record_name(), "my_record");
        assert_eq!(param.fields().len(), 2);
    }

    // Allocates 2^16+1 fields, which is pathologically slow under Miri.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn tuple_field_desc_to_tuple_binary_desc_too_many_nullable_columns() {
        let many_nullable: Vec<DatumDesc> = (0..=u16::MAX as usize + 1)
            .map(|i| nullable_i32_field(&format!("c{}", i)))
            .collect();
        let desc = TupleFieldDesc::new(many_nullable);
        let err = desc.to_tuple_binary_desc().unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Parse);
    }

    #[test]
    fn tuple_field_desc_to_tuple_binary_desc_normalizes_input() {
        let desc = TupleFieldDesc::new(vec![
            DatumDesc::new("s".to_string(), DataType::default_for(TypeFamily::String)),
            i32_field("i"),
        ]);
        let (binary_desc, mapping) = desc.to_tuple_binary_desc().unwrap();
        assert_eq!(binary_desc.field_count(), 2);
        // Normalized order places I32 before String.
        assert_eq!(binary_desc.get_field_desc(0).data_type(), TypeFamily::I32);
        assert_eq!(
            binary_desc.get_field_desc(1).data_type(),
            TypeFamily::String
        );
        // Mapping reflects original positions: original index 1 (i) became 0, original 0 (s) became 1.
        assert_eq!(mapping, vec![1, 0]);
    }
}
