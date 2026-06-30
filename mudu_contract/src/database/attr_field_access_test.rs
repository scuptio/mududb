#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::database::attr_field_access::{
        attr_get_binary, attr_get_value, attr_set_binary, attr_set_value, datum_from_value,
        field_from_binary, field_to_binary, field_to_value,
    };
    use mudu_type::dat_value::DatValue;

    #[test]
    fn field_binary_roundtrip() {
        let binary = field_to_binary(&42i32).unwrap();
        let restored: i32 = field_from_binary(&binary).unwrap();
        assert_eq!(restored, 42);
    }

    #[test]
    fn field_value_roundtrip() {
        let value = field_to_value(&42i32).unwrap();
        assert_eq!(*value.as_i32().unwrap(), 42);
        let restored: i32 = datum_from_value(&value).unwrap();
        assert_eq!(restored, 42);
    }

    #[test]
    fn datum_from_value_extracts_value() {
        let value = DatValue::from_i32(7);
        let datum: i32 = datum_from_value(&value).unwrap();
        assert_eq!(datum, 7);
    }

    #[test]
    fn attr_get_binary_some_and_none() {
        let some: Option<i32> = Some(42);
        let bin = attr_get_binary(&some).unwrap();
        assert!(bin.is_some());

        let none: Option<i32> = None;
        let bin = attr_get_binary(&none).unwrap();
        assert!(bin.is_none());
    }

    #[test]
    fn attr_set_binary_replaces_some_and_creates_none() {
        let mut attr: Option<i32> = Some(0);
        attr_set_binary(&mut attr, vec![0, 0, 0, 42]).unwrap();
        assert_eq!(attr.unwrap(), 42);

        let mut attr: Option<i32> = None;
        attr_set_binary(&mut attr, vec![0, 0, 0, 7]).unwrap();
        assert_eq!(attr.unwrap(), 7);
    }

    #[test]
    fn attr_get_value_some_and_none() {
        let some: Option<i32> = Some(42);
        let value = attr_get_value(&some).unwrap();
        assert_eq!(*value.unwrap().as_i32().unwrap(), 42);

        let none: Option<i32> = None;
        let value = attr_get_value(&none).unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn attr_set_value_replaces_some_and_creates_none() {
        let mut attr: Option<i32> = Some(0);
        attr_set_value(&mut attr, DatValue::from_i32(42)).unwrap();
        assert_eq!(attr.unwrap(), 42);

        let mut attr: Option<i32> = None;
        attr_set_value(&mut attr, DatValue::from_i32(7)).unwrap();
        assert_eq!(attr.unwrap(), 7);
    }
}
