#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::tuple::datum_desc::DatumDesc;
    use crate::tuple::enumerable_datum::EnumerableDatum;
    use crate::tuple::typed_bin::TypedBin;
    use mudu::error::ErrorCode;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::datum::DatumDyn;

    fn i32_desc() -> DatumDesc {
        DatumDesc::new("x".to_string(), DatType::new_no_param(DatTypeID::I32))
    }

    fn i64_desc() -> DatumDesc {
        DatumDesc::new("y".to_string(), DatType::new_no_param(DatTypeID::I64))
    }

    #[test]
    fn vec_dyn_datum_to_value_matching_length() {
        let typed = TypedBin::new(DatTypeID::I32, vec![0, 0, 0, 42]);
        let datum: &dyn DatumDyn = &typed;
        let values = [datum].to_value(&[i32_desc()]).unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(*values[0].as_i32().unwrap(), 42);
    }

    #[test]
    fn vec_dyn_datum_to_value_rejects_mismatched_length() {
        let typed = TypedBin::new(DatTypeID::I32, vec![0, 0, 0, 42]);
        let datum: &dyn DatumDyn = &typed;
        let err = [datum].to_value(&[]).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Parse);
    }

    #[test]
    fn vec_dyn_datum_to_binary_matching_length() {
        let typed = TypedBin::new(DatTypeID::I64, vec![0, 0, 0, 0, 0, 0, 0, 7]);
        let datum: &dyn DatumDyn = &typed;
        let bins = [datum].to_binary(&[i64_desc()]).unwrap();
        assert_eq!(bins.len(), 1);
        assert_eq!(bins[0], vec![0, 0, 0, 0, 0, 0, 0, 7]);
    }

    #[test]
    fn vec_dyn_datum_to_binary_rejects_mismatched_length() {
        let typed = TypedBin::new(DatTypeID::I32, vec![0, 0, 0, 42]);
        let datum: &dyn DatumDyn = &typed;
        let err = [datum].to_binary(&[i32_desc(), i32_desc()]).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Parse);
    }

    #[test]
    fn vec_dyn_datum_tuple_desc_with_matching_field_names() {
        let typed = TypedBin::new(DatTypeID::I32, vec![0, 0, 0, 42]);
        let datum: &dyn DatumDyn = &typed;
        let desc = [datum].tuple_desc(&["foo".to_string()]).unwrap();
        assert_eq!(desc.fields().len(), 1);
        assert_eq!(desc.fields()[0].name(), "foo");
        assert_eq!(desc.fields()[0].dat_type_id(), DatTypeID::I32);
    }

    #[test]
    fn vec_dyn_datum_tuple_desc_generates_default_names_when_length_mismatches() {
        let typed = TypedBin::new(DatTypeID::I32, vec![0, 0, 0, 42]);
        let datum: &dyn DatumDyn = &typed;
        let desc = [datum].tuple_desc(&[]).unwrap();
        assert_eq!(desc.fields().len(), 1);
        assert_eq!(desc.fields()[0].name(), "v_0");
    }

    #[test]
    fn vec_dyn_datum_tuple_desc_supports_multiple_fields() {
        let a = TypedBin::new(DatTypeID::I32, vec![0, 0, 0, 1]);
        let b = TypedBin::new(DatTypeID::I64, vec![0, 0, 0, 0, 0, 0, 0, 2]);
        let data: [&dyn DatumDyn; 2] = [&a, &b];
        let desc = data
            .tuple_desc(&["a".to_string(), "b".to_string()])
            .unwrap();
        assert_eq!(desc.fields().len(), 2);
        assert_eq!(desc.fields()[0].name(), "a");
        assert_eq!(desc.fields()[0].dat_type_id(), DatTypeID::I32);
        assert_eq!(desc.fields()[1].name(), "b");
        assert_eq!(desc.fields()[1].dat_type_id(), DatTypeID::I64);
    }
}
