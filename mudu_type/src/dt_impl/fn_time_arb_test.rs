#[cfg(test)]
mod tests {
    use crate::dat_type_id::DatTypeID;
    use crate::datum::DatumDyn;
    use crate::dt_impl::fn_time_arb::{
        fn_time_arb_dt_param, fn_time_arb_printable, fn_time_arb_val,
    };
    use arbitrary::Unstructured;

    const BYTES: &[u8] = &[0x00; 32];

    #[test]
    fn fn_time_arb_val_returns_time_dat_value() {
        let mut u = Unstructured::new(BYTES);
        let dt = fn_time_arb_dt_param(&mut u).unwrap();
        let mut u = Unstructured::new(BYTES);
        let value = fn_time_arb_val(&mut u, &dt).unwrap();
        assert_eq!(value.dat_type_id().unwrap(), DatTypeID::Time);
    }

    #[test]
    fn fn_time_arb_printable_returns_string() {
        let mut u = Unstructured::new(BYTES);
        let dt = fn_time_arb_dt_param(&mut u).unwrap();
        let mut u = Unstructured::new(BYTES);
        let printable = fn_time_arb_printable(&mut u, &dt).unwrap();
        assert!(!printable.is_empty());
    }

    #[test]
    fn fn_time_arb_dt_param_returns_time_type() {
        let mut u = Unstructured::new(BYTES);
        let dt = fn_time_arb_dt_param(&mut u).unwrap();
        assert_eq!(dt.dat_type_id(), DatTypeID::Time);
        let param = dt.as_time_param().unwrap();
        assert!(param.precision() <= 6);
    }
}
