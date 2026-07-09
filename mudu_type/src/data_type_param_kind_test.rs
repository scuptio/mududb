#[cfg(test)]
mod tests {
    use crate::data_type::DataType;
    use crate::data_type_param_array::DataTypeParamArray;
    use crate::data_type_param_kind::DataTypeParamKind;
    use crate::data_type_param_numeric::DataTypeParamNumeric;
    use crate::data_type_param_record::DataTypeParamRecord;
    use crate::data_type_param_string::DataTypeParamString;
    use crate::data_type_param_time::DataTypeParamTime;
    use crate::data_type_param_timestamp::DataTypeParamTimestamp;
    use crate::data_type_param_timestamptz::DataTypeParamTimestampTz;
    use crate::type_family::TypeFamily;
    use mudu::common::cmp_order::Order;
    use std::any::Any;

    #[test]
    fn type_family_and_name_for_each_variant() {
        let string = DataTypeParamKind::String(Box::new(DataTypeParamString::new(10)));
        assert_eq!(string.type_family(), TypeFamily::String);
        assert!(!string.name().is_empty());

        let numeric = DataTypeParamKind::Numeric(Box::new(DataTypeParamNumeric::new(10, 2)));
        assert_eq!(numeric.type_family(), TypeFamily::Numeric);

        let time = DataTypeParamKind::Time(Box::new(DataTypeParamTime::new(3)));
        assert_eq!(time.type_family(), TypeFamily::Time);

        let timestamp = DataTypeParamKind::Timestamp(Box::new(DataTypeParamTimestamp::new(3)));
        assert_eq!(timestamp.type_family(), TypeFamily::Timestamp);

        let timestamptz =
            DataTypeParamKind::TimestampTz(Box::new(DataTypeParamTimestampTz::new(3)));
        assert_eq!(timestamptz.type_family(), TypeFamily::TimestampTz);

        let record = DataTypeParamKind::Record(Box::new(DataTypeParamRecord::new(
            "R".to_string(),
            vec![("x".to_string(), DataType::new_no_param(TypeFamily::I32))],
        )));
        assert_eq!(record.type_family(), TypeFamily::Record);

        let array = DataTypeParamKind::Array(Box::new(DataTypeParamArray::new(
            DataType::new_no_param(TypeFamily::I32),
        )));
        assert_eq!(array.type_family(), TypeFamily::Array);
    }

    #[test]
    fn map_extracts_inner_value() {
        let kind = DataTypeParamKind::String(Box::new(DataTypeParamString::new(7)));
        let length = kind.map(|dyn_param| {
            (dyn_param as &dyn Any)
                .downcast_ref::<DataTypeParamString>()
                .unwrap()
                .length()
        });
        assert_eq!(length, 7);
    }

    #[test]
    fn as_dtp_dyn_returns_same_variant() {
        let kind = DataTypeParamKind::String(Box::new(DataTypeParamString::new(5)));
        let dyn_ref = kind.as_dtp_dyn();
        assert!(
            (dyn_ref as &dyn Any)
                .downcast_ref::<DataTypeParamString>()
                .is_some()
        );
    }

    #[test]
    fn as_param_methods() {
        let string = DataTypeParamKind::String(Box::new(DataTypeParamString::new(5)));
        assert!(string.as_string_param().is_some());
        assert!(string.as_numeric_param().is_none());
        assert_eq!(string.expect_string_param().length(), 5);
    }

    #[test]
    fn compare_same_variant() {
        let a = DataTypeParamKind::String(Box::new(DataTypeParamString::new(5)));
        let b = DataTypeParamKind::String(Box::new(DataTypeParamString::new(10)));
        assert_eq!(a.cmp_ord(&b).unwrap(), std::cmp::Ordering::Equal);
    }

    #[test]
    fn compare_different_variants() {
        let a = DataTypeParamKind::String(Box::new(DataTypeParamString::new(5)));
        let b = DataTypeParamKind::Numeric(Box::new(DataTypeParamNumeric::new(10, 2)));
        assert_eq!(
            a.cmp_ord(&b).unwrap(),
            TypeFamily::String.cmp(&TypeFamily::Numeric)
        );
    }
}
