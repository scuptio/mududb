#[cfg(test)]
mod tests {
    use crate::dat_type::DatType;
    use crate::dat_type_id::DatTypeID;
    use crate::dtp_array::DTPArray;
    use crate::dtp_kind::DTPKind;
    use crate::dtp_numeric::DTPNumeric;
    use crate::dtp_object::DTPRecord;
    use crate::dtp_string::DTPString;
    use crate::dtp_time::DTPTime;
    use crate::dtp_timestamp::DTPTimestamp;
    use crate::dtp_timestamptz::DTPTimestampTz;
    use mudu::common::cmp_order::Order;
    use std::any::Any;

    #[test]
    fn dat_type_id_and_name_for_each_variant() {
        let string = DTPKind::String(Box::new(DTPString::new(10)));
        assert_eq!(string.dat_type_id(), DatTypeID::String);
        assert!(!string.name().is_empty());

        let numeric = DTPKind::Numeric(Box::new(DTPNumeric::new(10, 2)));
        assert_eq!(numeric.dat_type_id(), DatTypeID::Numeric);

        let time = DTPKind::Time(Box::new(DTPTime::new(3)));
        assert_eq!(time.dat_type_id(), DatTypeID::Time);

        let timestamp = DTPKind::Timestamp(Box::new(DTPTimestamp::new(3)));
        assert_eq!(timestamp.dat_type_id(), DatTypeID::Timestamp);

        let timestamptz = DTPKind::TimestampTz(Box::new(DTPTimestampTz::new(3)));
        assert_eq!(timestamptz.dat_type_id(), DatTypeID::TimestampTz);

        let record = DTPKind::Record(Box::new(DTPRecord::new(
            "R".to_string(),
            vec![("x".to_string(), DatType::new_no_param(DatTypeID::I32))],
        )));
        assert_eq!(record.dat_type_id(), DatTypeID::Record);

        let array = DTPKind::Array(Box::new(DTPArray::new(DatType::new_no_param(
            DatTypeID::I32,
        ))));
        assert_eq!(array.dat_type_id(), DatTypeID::Array);
    }

    #[test]
    fn map_extracts_inner_value() {
        let kind = DTPKind::String(Box::new(DTPString::new(7)));
        let length = kind.map(|dyn_param| {
            (dyn_param as &dyn Any)
                .downcast_ref::<DTPString>()
                .unwrap()
                .length()
        });
        assert_eq!(length, 7);
    }

    #[test]
    fn as_dtp_dyn_returns_same_variant() {
        let kind = DTPKind::String(Box::new(DTPString::new(5)));
        let dyn_ref = kind.as_dtp_dyn();
        assert!((dyn_ref as &dyn Any).downcast_ref::<DTPString>().is_some());
    }

    #[test]
    fn as_param_methods() {
        let string = DTPKind::String(Box::new(DTPString::new(5)));
        assert!(string.as_string_param().is_some());
        assert!(string.as_numeric_param().is_none());
        assert_eq!(string.expect_string_param().length(), 5);
    }

    #[test]
    fn compare_same_variant() {
        let a = DTPKind::String(Box::new(DTPString::new(5)));
        let b = DTPKind::String(Box::new(DTPString::new(10)));
        assert_eq!(a.cmp_ord(&b).unwrap(), std::cmp::Ordering::Equal);
    }

    #[test]
    fn compare_different_variants() {
        let a = DTPKind::String(Box::new(DTPString::new(5)));
        let b = DTPKind::Numeric(Box::new(DTPNumeric::new(10, 2)));
        assert_eq!(
            a.cmp_ord(&b).unwrap(),
            DatTypeID::String.cmp(&DatTypeID::Numeric)
        );
    }
}
