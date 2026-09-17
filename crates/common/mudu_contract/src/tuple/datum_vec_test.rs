#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::tuple::datum_desc::DatumDesc;
    use crate::tuple::datum_vec::{datum_vec_to_bin_vec, datum_vec_to_value_vec};
    use crate::tuple::typed_bin::TypedBin;
    use mudu::common::result::RS;
    use mudu::error::ErrorCode;
    use mudu::mudu_error;
    use mudu_type::data_binary::DataBinary;
    use mudu_type::data_textual::DataTextual;
    use mudu_type::data_type::DataType;
    use mudu_type::data_value::DataValue;
    use mudu_type::datum::DatumDyn;
    use mudu_type::type_family::TypeFamily;

    fn i32_desc() -> DatumDesc {
        DatumDesc::new("x".to_string(), DataType::new_no_param(TypeFamily::I32))
    }

    #[test]
    fn datum_vec_to_bin_vec_roundtrip() {
        let typed = TypedBin::new(TypeFamily::I32, vec![0, 0, 0, 42]);
        let datum: &dyn DatumDyn = &typed;
        let bins = datum_vec_to_bin_vec(&[datum], &[i32_desc()]).unwrap();
        assert_eq!(bins.len(), 1);
        assert_eq!(bins[0], vec![0, 0, 0, 42]);
    }

    #[test]
    fn datum_vec_to_value_vec_roundtrip() {
        let typed = TypedBin::new(TypeFamily::I32, vec![0, 0, 0, 42]);
        let datum: &dyn DatumDyn = &typed;
        let values = datum_vec_to_value_vec(&[datum], &[i32_desc()]).unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(*values[0].as_i32().unwrap(), 42);
    }

    #[test]
    fn datum_vec_rejects_mismatched_lengths() {
        let typed = TypedBin::new(TypeFamily::I32, vec![0, 0, 0, 42]);
        let datum: &dyn DatumDyn = &typed;
        let err = datum_vec_to_bin_vec(&[datum], &[]).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);

        let err = datum_vec_to_value_vec(&[datum], &[]).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }

    #[derive(Clone, Debug)]
    struct FailingDatum;

    impl DatumDyn for FailingDatum {
        fn type_family(&self) -> RS<TypeFamily> {
            Ok(TypeFamily::I32)
        }

        fn to_binary(&self, _: &DataType) -> RS<DataBinary> {
            Err(mudu_error!(
                ErrorCode::TypeConversionFailed,
                "to_binary fails"
            ))
        }

        fn to_textual(&self, _: &DataType) -> RS<DataTextual> {
            Err(mudu_error!(
                ErrorCode::TypeConversionFailed,
                "to_textual fails"
            ))
        }

        fn to_value(&self, _: &DataType) -> RS<DataValue> {
            Err(mudu_error!(
                ErrorCode::TypeConversionFailed,
                "to_value fails"
            ))
        }

        fn clone_boxed(&self) -> Box<dyn DatumDyn> {
            Box::new(self.clone())
        }
    }

    #[test]
    fn datum_vec_to_bin_vec_propagates_conversion_error() {
        let datum: &dyn DatumDyn = &FailingDatum;
        let err = datum_vec_to_bin_vec(&[datum], &[i32_desc()]).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }

    #[test]
    fn datum_vec_to_value_vec_propagates_conversion_error() {
        let datum: &dyn DatumDyn = &FailingDatum;
        let err = datum_vec_to_value_vec(&[datum], &[i32_desc()]).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }
}
