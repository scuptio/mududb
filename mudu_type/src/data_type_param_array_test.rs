#[cfg(test)]
mod tests {
    use crate::data_type::DataType;
    use crate::data_type_param::DataTypeParamDyn;
    use crate::data_type_param_array::DataTypeParamArray;
    use crate::type_family::TypeFamily;
    use mudu::common::cmp_order::Order;

    #[test]
    fn array_constructors_and_accessors() {
        let inner = DataType::new_no_param(TypeFamily::I64);
        let array = DataTypeParamArray::new(inner.clone());
        assert_eq!(array.data_type().type_family(), TypeFamily::I64);
        assert_eq!(array.into_data_type().type_family(), TypeFamily::I64);
    }

    #[test]
    fn array_default_uses_i32() {
        let array = DataTypeParamArray::default();
        assert_eq!(array.data_type().type_family(), TypeFamily::I32);
    }

    #[test]
    fn array_name_includes_array_and_inner_type() {
        let array = DataTypeParamArray::new(DataType::new_no_param(TypeFamily::I32));
        let name = array.name().to_lowercase();
        assert!(name.contains("array"));
        assert!(name.contains("i32") || name.contains("int"));
    }

    #[test]
    fn array_json_roundtrip() {
        let array = DataTypeParamArray::new(DataType::new_no_param(TypeFamily::I64));
        let json = array.se_to_json().unwrap();
        let mut restored = DataTypeParamArray::default();
        restored.de_from_json(&json).unwrap();
        assert_eq!(restored.data_type().type_family(), TypeFamily::I64);
    }

    #[test]
    fn array_compare_orders_by_inner_type() {
        let a = DataTypeParamArray::new(DataType::new_no_param(TypeFamily::I32));
        let b = DataTypeParamArray::new(DataType::new_no_param(TypeFamily::I64));
        assert_eq!(a.cmp_ord(&b).unwrap(), std::cmp::Ordering::Less);
    }
}
