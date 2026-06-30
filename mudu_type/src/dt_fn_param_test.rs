#[cfg(test)]
mod tests {
    use crate::dat_type_id::DatTypeID;
    use crate::dt_fn_param::FnParam;

    #[test]
    fn fn_param_display_and_debug_use_derived_format() {
        let param = FnParam {
            input: DatTypeID::String.opt_fn_param().as_ref().unwrap().input,
            default: Some(DatTypeID::String.fn_param_default().unwrap()),
        };

        let display = format!("{}", param);
        let debug = format!("{:?}", param);
        assert_eq!(display, debug);
        assert!(display.contains("FnParam"));
        assert!(display.contains("input"));
        assert!(display.contains("default"));
    }

    #[test]
    fn fn_param_can_be_constructed_without_default() {
        let param = FnParam {
            input: DatTypeID::Numeric.opt_fn_param().as_ref().unwrap().input,
            default: None,
        };

        assert!(format!("{:?}", param).contains("None"));
    }

    #[test]
    fn fn_param_default_produces_expected_type() {
        let default_fn = DatTypeID::String.fn_param_default().unwrap();
        let dt = default_fn();
        assert_eq!(dt.dat_type_id(), DatTypeID::String);
    }

    #[test]
    fn fn_param_input_parses_registered_param() {
        let input_fn = DatTypeID::Numeric.opt_fn_param().as_ref().unwrap().input;
        let dt = input_fn("{\"precision\":10,\"scale\":2}").unwrap();
        assert_eq!(dt.dat_type_id(), DatTypeID::Numeric);
        assert_eq!(dt.expect_numeric_param().precision(), 10);
        assert_eq!(dt.expect_numeric_param().scale(), 2);
    }

    #[test]
    fn fn_param_clones_and_equality_by_debug() {
        let param = FnParam {
            input: DatTypeID::String.opt_fn_param().as_ref().unwrap().input,
            default: Some(DatTypeID::String.fn_param_default().unwrap()),
        };
        let cloned = param.clone();
        assert_eq!(format!("{:?}", param), format!("{:?}", cloned));
    }
}
