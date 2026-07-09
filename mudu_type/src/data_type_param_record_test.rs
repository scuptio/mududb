#[cfg(test)]
mod tests {
    use crate::data_type::DataType;
    use crate::data_type_param::DataTypeParamDyn;
    use crate::data_type_param_record::DataTypeParamRecord;
    use crate::type_family::TypeFamily;
    use mudu::common::cmp_order::Order;

    #[test]
    fn record_constructors_and_accessors() {
        let fields = vec![("x".to_string(), DataType::new_no_param(TypeFamily::I32))];
        let record = DataTypeParamRecord::new("R".to_string(), fields.clone());
        assert_eq!(record.record_name(), "R");
        assert_eq!(record.fields().len(), 1);
        assert_eq!(record.fields()[0].0, "x");
        let (name, fields_out) = record.into();
        assert_eq!(name, "R");
        assert_eq!(fields_out.len(), 1);
    }

    #[test]
    fn record_default_is_empty() {
        let record = DataTypeParamRecord::default();
        assert!(record.record_name().is_empty());
        assert!(record.fields().is_empty());
    }

    #[test]
    fn record_json_roundtrip() {
        let fields = vec![("x".to_string(), DataType::new_no_param(TypeFamily::I64))];
        let record = DataTypeParamRecord::new("R".to_string(), fields);
        let json = record.se_to_json().unwrap();
        let mut restored = DataTypeParamRecord::default();
        restored.de_from_json(&json).unwrap();
        assert_eq!(restored.record_name(), "R");
        assert_eq!(restored.fields().len(), 1);
    }

    #[test]
    fn record_compare_by_name_then_field_count() {
        let a = DataTypeParamRecord::new("A".to_string(), vec![]);
        let b = DataTypeParamRecord::new("B".to_string(), vec![]);
        assert_eq!(a.cmp_ord(&b).unwrap(), std::cmp::Ordering::Equal);

        let a2 = DataTypeParamRecord::new("A".to_string(), vec![]);
        let b2 = DataTypeParamRecord::new(
            "A".to_string(),
            vec![("x".to_string(), DataType::new_no_param(TypeFamily::I32))],
        );
        assert_eq!(a2.cmp_ord(&b2).unwrap(), std::cmp::Ordering::Equal);

        let fewer = DataTypeParamRecord::new("A".to_string(), vec![]);
        let more = DataTypeParamRecord::new(
            "B".to_string(),
            vec![("x".to_string(), DataType::new_no_param(TypeFamily::I32))],
        );
        assert_eq!(fewer.cmp_ord(&more).unwrap(), std::cmp::Ordering::Less);
    }
}
