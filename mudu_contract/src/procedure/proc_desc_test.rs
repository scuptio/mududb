#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::procedure::proc_desc::ProcDesc;
    use crate::tuple::datum_desc::DatumDesc;
    use crate::tuple::tuple_datum::TupleDatum;
    use crate::tuple::tuple_field_desc::TupleFieldDesc;
    use mudu::error::ErrorCode;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dtp_kind::DTPKind;
    use mudu_type::dtp_numeric::DTPNumeric;

    fn sample_desc() -> ProcDesc {
        let param_desc = <(i32, i32, i64)>::tuple_desc_static(&[]);
        let return_desc = <(i32, String)>::tuple_desc_static(&[]);
        ProcDesc::new(
            "module".to_string(),
            "proc".to_string(),
            param_desc,
            return_desc,
            false,
        )
    }

    #[test]
    fn test_proc_desc_serialization() {
        let proc_desc = sample_desc();

        let path = format!(
            "{}/proc_desc.toml",
            mudu_sys::env_var::temp_dir().to_str().unwrap()
        );
        println!("Test file path: {}", path);

        proc_desc.write_to_file(&path).unwrap();

        let loaded_desc = ProcDesc::from_path(&path).unwrap();
        let param_json = loaded_desc.default_param_json().unwrap();
        let return_json = loaded_desc.default_return_json().unwrap();
        println!("parameter:{}", param_json);
        println!("return:{}", return_json);

        assert!(param_json.to_string().contains("field_0"));
        assert!(return_json.to_string().contains("field_1"));

        let _ = mudu_sys::fs::sync::sync_remove_file(&path);
    }

    #[test]
    fn getters() {
        let desc = sample_desc();
        assert_eq!(desc.module_name(), "module");
        assert_eq!(desc.proc_name(), "proc");
        assert!(!desc.is_async());
        assert_eq!(desc.param_desc().fields().len(), 3);
        assert_eq!(desc.return_desc().fields().len(), 2);
    }

    #[test]
    fn to_toml_str_roundtrip() {
        let desc = sample_desc();
        let s = desc.to_toml_str().unwrap();
        let loaded: ProcDesc = toml::from_str(&s).unwrap();
        assert_eq!(loaded.module_name(), desc.module_name());
        assert_eq!(loaded.proc_name(), desc.proc_name());
        assert_eq!(
            loaded.param_desc().fields().len(),
            desc.param_desc().fields().len()
        );
        assert_eq!(
            loaded.return_desc().fields().len(),
            desc.return_desc().fields().len()
        );
    }

    #[test]
    fn default_param_json_contains_fields() {
        let desc = sample_desc();
        let json = desc.default_param_json().unwrap();
        let s = json.to_string();
        assert!(s.contains("field_0"));
        assert!(s.contains("field_1"));
        assert!(s.contains("field_2"));
    }

    #[test]
    fn default_return_json_contains_fields() {
        let desc = sample_desc();
        let json = desc.default_return_json().unwrap();
        let s = json.to_string();
        assert!(s.contains("field_0"));
        assert!(s.contains("field_1"));
    }

    fn numeric_zero_precision_dat_type() -> DatType {
        DatType::from_id_param(
            DatTypeID::Numeric,
            Some(DTPKind::Numeric(Box::new(DTPNumeric::new(0, 0)))),
        )
    }

    #[test]
    fn default_param_json_propagates_default_error() {
        let param_desc = TupleFieldDesc::new(vec![DatumDesc::new(
            "n".to_string(),
            numeric_zero_precision_dat_type(),
        )]);
        let return_desc = <() as TupleDatum>::tuple_desc_static(&[]);
        let desc = ProcDesc::new(
            "m".to_string(),
            "p".to_string(),
            param_desc,
            return_desc,
            false,
        );
        let err = desc.default_param_json().unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }

    #[test]
    fn from_path_rejects_invalid_toml() {
        let path = format!(
            "{}/proc_desc_bad.toml",
            mudu_sys::env_var::temp_dir().to_str().unwrap()
        );
        mudu_sys::fs::sync::sync_write(path.as_str(), b"not valid toml [[").unwrap();
        let err = ProcDesc::from_path(&path).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
        let _ = mudu_sys::fs::sync::sync_remove_file(&path);
    }
}
