#[cfg(test)]
mod tests {
    use crate::data_type::DataType;
    use crate::data_type_param::DataTypeParamDyn;
    use crate::data_type_param_struct::DataTypeParamStruct;
    use crate::type_family::TypeFamily;
    use mudu::common::cmp_order::Order;

    #[test]
    fn record_constructors_and_accessors() {
        let fields = vec![("x".to_string(), DataType::new_no_param(TypeFamily::I32))];
        let record = DataTypeParamStruct::new("R".to_string(), fields.clone());
        assert_eq!(record.struct_name(), "R");
        assert_eq!(record.fields().len(), 1);
        assert_eq!(record.fields()[0].0, "x");
        let (name, fields_out) = record.into();
        assert_eq!(name, "R");
        assert_eq!(fields_out.len(), 1);
    }

    #[test]
    fn record_default_is_empty() {
        let record = DataTypeParamStruct::<DataType>::default();
        assert!(record.struct_name().is_empty());
        assert!(record.fields().is_empty());
    }

    #[test]
    fn record_json_roundtrip() {
        let fields = vec![("x".to_string(), DataType::new_no_param(TypeFamily::I64))];
        let record = DataTypeParamStruct::new("R".to_string(), fields);
        let json = record.se_to_json().unwrap();
        let mut restored = DataTypeParamStruct::<DataType>::default();
        restored.de_from_json(&json).unwrap();
        assert_eq!(restored.struct_name(), "R");
        assert_eq!(restored.fields().len(), 1);
    }

    #[test]
    fn record_compare_by_name_then_field_count() {
        let a = DataTypeParamStruct::<DataType>::new("A".to_string(), vec![]);
        let b = DataTypeParamStruct::<DataType>::new("B".to_string(), vec![]);
        assert_eq!(a.cmp_ord(&b).unwrap(), std::cmp::Ordering::Equal);

        let a2 = DataTypeParamStruct::<DataType>::new("A".to_string(), vec![]);
        let b2 = DataTypeParamStruct::new(
            "A".to_string(),
            vec![("x".to_string(), DataType::new_no_param(TypeFamily::I32))],
        );
        assert_eq!(a2.cmp_ord(&b2).unwrap(), std::cmp::Ordering::Equal);

        let fewer = DataTypeParamStruct::<DataType>::new("A".to_string(), vec![]);
        let more = DataTypeParamStruct::new(
            "B".to_string(),
            vec![("x".to_string(), DataType::new_no_param(TypeFamily::I32))],
        );
        assert_eq!(fewer.cmp_ord(&more).unwrap(), std::cmp::Ordering::Less);
    }
    #[test]
    fn cloned_record_parameter_retains_schema_and_failed_decode_is_atomic() {
        let record = DataTypeParamStruct::new(
            "R".into(),
            vec![("id".into(), DataType::new_no_param(TypeFamily::I64))],
        );
        let clone = record.clone_boxed();
        assert_eq!(clone.name(), "R");
        let mut restored = DataTypeParamStruct::<DataType>::default();
        restored.de_from_json(&clone.se_to_json().unwrap()).unwrap();
        assert!(
            restored
                .de_from_json(r#"{"name":"bad","field":false}"#)
                .is_err()
        );
        assert_eq!(restored.struct_name(), "R");
        assert_eq!(restored.fields()[0].0, "id");
        assert_eq!(restored.fields()[0].1.type_family(), TypeFamily::I64);
    }
}
