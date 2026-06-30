#[cfg(test)]
mod tests {
    use crate::dat_type_id::DatTypeID;
    use crate::datum::DatumDyn;
    use crate::dt_impl::fn_date_arb::{
        fn_date_arb_dt_param, fn_date_arb_printable, fn_date_arb_val,
    };
    use arbitrary::Unstructured;

    const BYTES: &[u8] = &[0x00; 32];

    #[test]
    fn fn_date_arb_val_returns_date_dat_value() {
        let mut u = Unstructured::new(BYTES);
        let dt = fn_date_arb_dt_param(&mut u).unwrap();
        let mut u = Unstructured::new(BYTES);
        let value = fn_date_arb_val(&mut u, &dt).unwrap();
        assert_eq!(value.dat_type_id().unwrap(), DatTypeID::Date);
    }

    #[test]
    fn fn_date_arb_printable_returns_string() {
        let mut u = Unstructured::new(BYTES);
        let dt = fn_date_arb_dt_param(&mut u).unwrap();
        let mut u = Unstructured::new(BYTES);
        let printable = fn_date_arb_printable(&mut u, &dt).unwrap();
        assert!(!printable.is_empty());
    }

    #[test]
    fn fn_date_arb_dt_param_returns_date_type() {
        let mut u = Unstructured::new(BYTES);
        let dt = fn_date_arb_dt_param(&mut u).unwrap();
        assert_eq!(dt.dat_type_id(), DatTypeID::Date);
    }
}
