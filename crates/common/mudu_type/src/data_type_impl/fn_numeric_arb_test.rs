#[cfg(test)]
mod tests {
    use crate::data_type_impl::fn_numeric_arb::{
        fn_numeric_arb_data_type_param, fn_numeric_arb_printable, fn_numeric_arb_val,
    };
    use crate::datum::DatumDyn;
    use crate::type_family::TypeFamily;
    use arbitrary::Unstructured;

    const BYTES: &[u8] = &[0x00; 32];

    #[test]
    fn fn_numeric_arb_val_returns_numeric_data_value() {
        let mut u = Unstructured::new(BYTES);
        let dt = fn_numeric_arb_data_type_param(&mut u).unwrap();
        let mut u = Unstructured::new(BYTES);
        let value = fn_numeric_arb_val(&mut u, &dt).unwrap();
        assert_eq!(value.type_family().unwrap(), TypeFamily::Numeric);
    }

    #[test]
    fn fn_numeric_arb_printable_returns_string() {
        let mut u = Unstructured::new(BYTES);
        let dt = fn_numeric_arb_data_type_param(&mut u).unwrap();
        let mut u = Unstructured::new(BYTES);
        let printable = fn_numeric_arb_printable(&mut u, &dt).unwrap();
        assert!(!printable.is_empty());
    }

    #[test]
    fn fn_numeric_arb_dt_param_returns_numeric_type() {
        let mut u = Unstructured::new(BYTES);
        let dt = fn_numeric_arb_data_type_param(&mut u).unwrap();
        assert_eq!(dt.type_family(), TypeFamily::Numeric);
        let param = dt.as_numeric_param().unwrap();
        assert!(param.precision() > 0 && param.precision() <= 18);
        assert!(param.scale() <= param.precision());
    }
}
