#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::tuple::typed_bin::TypedBin;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::datum::DatumDyn;

    fn i32_type() -> DatType {
        DatType::new_no_param(DatTypeID::I32)
    }

    #[test]
    fn typed_bin_new_and_debug() {
        let bin = TypedBin::new(DatTypeID::I32, vec![0, 0, 0, 7]);
        let debug = format!("{:?}", bin);
        assert!(debug.contains("I32"));
    }

    #[test]
    fn typed_bin_dat_type_id_and_clone() {
        let bin = TypedBin::new(DatTypeID::I64, vec![0; 8]);
        assert_eq!(bin.dat_type_id().unwrap(), DatTypeID::I64);
        let cloned: Box<dyn DatumDyn> = bin.clone_boxed();
        assert_eq!(cloned.dat_type_id().unwrap(), DatTypeID::I64);
    }

    #[test]
    fn typed_bin_to_binary() {
        let bin = TypedBin::new(DatTypeID::I32, vec![0, 0, 0, 9]);
        let bytes = bin.to_binary(&i32_type()).unwrap();
        assert_eq!(bytes.as_ref(), &[0, 0, 0, 9]);
    }

    #[test]
    fn typed_bin_to_value_roundtrip() {
        let ty = i32_type();
        let original = TypedBin::new(DatTypeID::I32, vec![0, 0, 0, 42]);
        let value = original.to_value(&ty).unwrap();
        assert_eq!(*value.as_i32().unwrap(), 42);
    }

    #[test]
    fn typed_bin_to_textual() {
        let ty = i32_type();
        let bin = TypedBin::new(DatTypeID::I32, vec![0, 0, 0, 42]);
        let text = bin.to_textual(&ty).unwrap();
        assert_eq!(text.as_ref(), "42");
    }

    #[test]
    fn typed_bin_to_textual_propagates_recv_error() {
        let ty = i32_type();
        let bin = TypedBin::new(DatTypeID::I32, vec![0, 0]);
        let err = bin.to_textual(&ty).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::TypeConversionFailed);
    }

    #[test]
    fn typed_bin_to_value_propagates_recv_error() {
        let ty = i32_type();
        let bin = TypedBin::new(DatTypeID::I32, vec![0, 0]);
        let err = bin.to_value(&ty).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::TypeConversionFailed);
    }
}
