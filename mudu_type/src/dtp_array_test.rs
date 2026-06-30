#[cfg(test)]
mod tests {
    use crate::dat_type::DatType;
    use crate::dat_type_id::DatTypeID;
    use crate::dt_param::DTPDyn;
    use crate::dtp_array::DTPArray;
    use mudu::common::cmp_order::Order;

    #[test]
    fn array_constructors_and_accessors() {
        let inner = DatType::new_no_param(DatTypeID::I64);
        let array = DTPArray::new(inner.clone());
        assert_eq!(array.dat_type().dat_type_id(), DatTypeID::I64);
        assert_eq!(array.into_dat_type().dat_type_id(), DatTypeID::I64);
    }

    #[test]
    fn array_default_uses_i32() {
        let array = DTPArray::default();
        assert_eq!(array.dat_type().dat_type_id(), DatTypeID::I32);
    }

    #[test]
    fn array_name_includes_array_and_inner_type() {
        let array = DTPArray::new(DatType::new_no_param(DatTypeID::I32));
        let name = array.name().to_lowercase();
        assert!(name.contains("array"));
        assert!(name.contains("i32") || name.contains("int"));
    }

    #[test]
    fn array_json_roundtrip() {
        let array = DTPArray::new(DatType::new_no_param(DatTypeID::I64));
        let json = array.se_to_json().unwrap();
        let mut restored = DTPArray::default();
        restored.de_from_json(&json).unwrap();
        assert_eq!(restored.dat_type().dat_type_id(), DatTypeID::I64);
    }

    #[test]
    fn array_compare_orders_by_inner_type() {
        let a = DTPArray::new(DatType::new_no_param(DatTypeID::I32));
        let b = DTPArray::new(DatType::new_no_param(DatTypeID::I64));
        assert_eq!(a.cmp_ord(&b).unwrap(), std::cmp::Ordering::Less);
    }
}
