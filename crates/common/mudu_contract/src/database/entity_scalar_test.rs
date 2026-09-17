#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::database::entity::Entity;
    use crate::tuple::tuple_field::TupleField;
    use crate::tuple::tuple_value::TupleValue;
    use mudu::error::ErrorCode;
    use mudu_type::data_value::DataValue;

    #[test]
    fn i32_entity_lifecycle() {
        assert_eq!(<i32 as Entity>::table_name(), "object_i32");
        assert_eq!(<i32 as Entity>::tuple_desc().fields().len(), 1);
        assert_eq!(
            <i32 as Entity>::tuple_desc().fields()[0].name(),
            "field_i32"
        );

        let e = 7i32;
        let tuple = e.to_tuple().unwrap();
        let restored = i32::from_tuple(&tuple).unwrap();
        assert_eq!(restored, 7);

        let value = e.to_tuple_value().unwrap();
        let restored = i32::from_tuple_value(&value).unwrap();
        assert_eq!(restored, 7);
    }

    #[test]
    fn string_entity_lifecycle() {
        assert_eq!(<String as Entity>::table_name(), "object_string");

        let e = "hello".to_string();
        let tuple = e.to_tuple().unwrap();
        let restored = String::from_tuple(&tuple).unwrap();
        assert_eq!(restored, "hello");

        let value = e.to_tuple_value().unwrap();
        let restored = String::from_tuple_value(&value).unwrap();
        assert_eq!(restored, "hello");
    }

    #[test]
    fn i64_entity_lifecycle() {
        assert_eq!(<i64 as Entity>::table_name(), "object_i64");
        let e = 123i64;
        let restored = i64::from_tuple(&e.to_tuple().unwrap()).unwrap();
        assert_eq!(restored, 123);
    }

    #[test]
    fn f32_entity_lifecycle() {
        assert_eq!(<f32 as Entity>::table_name(), "object_f32");
        let e = 1.5f32;
        let restored = f32::from_tuple(&e.to_tuple().unwrap()).unwrap();
        assert_eq!(restored, 1.5);
    }

    #[test]
    fn f64_entity_lifecycle() {
        assert_eq!(<f64 as Entity>::table_name(), "object_f64");
        let e = 2.5f64;
        let restored = f64::from_tuple(&e.to_tuple().unwrap()).unwrap();
        assert_eq!(restored, 2.5);
    }

    #[test]
    fn scalar_from_tuple_rejects_length_mismatch() {
        let tuple = TupleField::new(vec![vec![0, 0, 0, 1], vec![0, 0, 0, 2]]);
        let err = i32::from_tuple(&tuple).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidType);
    }

    #[test]
    fn scalar_from_tuple_rejects_null_field() {
        let tuple = TupleField::new_nullable(vec![None]);
        let err = i32::from_tuple(&tuple).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidType);
    }

    #[test]
    fn scalar_from_tuple_value_rejects_length_mismatch() {
        let row = TupleValue::from(vec![DataValue::from_i32(1), DataValue::from_i32(2)]);
        let err = i32::from_tuple_value(&row).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidType);
    }
}
