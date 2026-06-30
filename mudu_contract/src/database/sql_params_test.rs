#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::database::sql_params::SQLParams;
    use crate::tuple::datum_desc::DatumDesc;
    use crate::tuple::typed_bin::TypedBin;
    use mudu::error::ErrorCode;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::datum::DatumDyn;

    fn i32_desc() -> DatumDesc {
        DatumDesc::new("v".to_string(), DatType::new_no_param(DatTypeID::I32))
    }

    #[test]
    fn param_to_binary_rejects_desc_size_mismatch() {
        let params = (1i32, 2i32);
        let desc = vec![i32_desc()];
        let err = params.param_to_binary(&desc).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Parse);
    }

    #[test]
    fn unit_params_has_zero_size() {
        let params = ();
        assert_eq!(params.size(), 0);
        assert!(params.get_idx(0).is_none());
        let desc = params.param_tuple_desc().unwrap();
        assert!(desc.fields().is_empty());
    }

    #[test]
    fn single_datum_param_roundtrip() {
        let params = 42i32;
        assert_eq!(params.size(), 1);
        let datum = params.get_idx(0).unwrap();
        assert_eq!(datum.dat_type_id().unwrap(), DatTypeID::I32);
        // Single-value impl returns the same datum for any index.
        assert!(params.get_idx(100).is_some());

        let desc = vec![i32_desc()];
        let binaries = params.param_to_binary(&desc).unwrap();
        assert_eq!(binaries.len(), 1);
        assert_eq!(binaries[0], 42i32.to_be_bytes().to_vec());
    }

    #[test]
    fn single_element_tuple_params_roundtrip() {
        let params = (42i32,);
        assert_eq!(params.size(), 1);
        let desc = params.param_tuple_desc().unwrap();
        assert_eq!(desc.fields().len(), 1);

        let datum_descs = desc.fields().to_vec();
        let binaries = params.param_to_binary(&datum_descs).unwrap();
        assert_eq!(binaries.len(), 1);
        assert!(params.get_idx(0).is_some());
    }

    #[test]
    fn tuple_params_roundtrip() {
        let params = (1i32, "hello".to_string());
        assert_eq!(params.size(), 2);
        let desc = params.param_tuple_desc().unwrap();
        assert_eq!(desc.fields().len(), 2);

        let datum_descs = desc.fields().to_vec();
        let binaries = params.param_to_binary(&datum_descs).unwrap();
        assert_eq!(binaries.len(), 2);
        assert!(params.get_idx(10).is_none());
    }

    #[test]
    fn vec_box_datum_params_roundtrip() {
        let params: Vec<Box<dyn DatumDyn>> = vec![
            Box::new(TypedBin::new(DatTypeID::I32, vec![0, 0, 0, 1])),
            Box::new(TypedBin::new(DatTypeID::I64, vec![0, 0, 0, 0, 0, 0, 0, 2])),
        ];
        assert_eq!(params.size(), 2);
        let desc = params.param_tuple_desc().unwrap();
        assert_eq!(desc.fields().len(), 2);

        let datum_descs = desc.fields().to_vec();
        let binaries = params.param_to_binary(&datum_descs).unwrap();
        assert_eq!(binaries.len(), 2);
        assert!(params.get_idx(5).is_none());
    }
}
