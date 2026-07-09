#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::database::entity::Entity;
    use mudu_type::data_value::DataValue;

    #[test]
    fn i32_entity_lifecycle() {
        let mut e: i32 = Entity::new_empty();
        assert_eq!(e, 0);

        assert_eq!(<i32 as Entity>::object_name(), "object_i32");
        assert_eq!(<i32 as Entity>::tuple_desc().fields().len(), 1);

        e.set_field_binary("field_i32", vec![0, 0, 0, 42]).unwrap();
        assert_eq!(
            e.get_field_binary("field_i32").unwrap().unwrap(),
            vec![0, 0, 0, 42]
        );

        e.set_field_value("field_i32", DataValue::from_i32(7))
            .unwrap();
        assert_eq!(
            *e.get_field_value("field_i32")
                .unwrap()
                .unwrap()
                .as_i32()
                .unwrap(),
            7
        );

        let tuple = e.to_tuple().unwrap();
        let restored = i32::from_tuple(&tuple).unwrap();
        assert_eq!(restored, 7);
    }

    #[test]
    fn string_entity_lifecycle() {
        let mut e: String = Entity::new_empty();
        assert!(e.is_empty());

        assert_eq!(<String as Entity>::object_name(), "object_string");

        e.set_field_value("field_string", DataValue::from_string("hello".to_string()))
            .unwrap();
        assert_eq!(e, "hello");

        let tuple = e.to_tuple().unwrap();
        let restored = String::from_tuple(&tuple).unwrap();
        assert_eq!(restored, "hello");
    }

    #[test]
    fn i64_entity_lifecycle() {
        let mut e: i64 = Entity::new_empty();
        assert_eq!(e, 0);

        assert_eq!(<i64 as Entity>::object_name(), "object_i64");

        e.set_field_value("field_i64", DataValue::from_i64(123))
            .unwrap();
        assert_eq!(e, 123);

        let tuple = e.to_tuple().unwrap();
        let restored = i64::from_tuple(&tuple).unwrap();
        assert_eq!(restored, 123);
    }

    #[test]
    fn f32_entity_lifecycle() {
        let mut e: f32 = Entity::new_empty();
        assert_eq!(e, 0.0);

        assert_eq!(<f32 as Entity>::object_name(), "object_f32");

        e.set_field_value("field_f32", DataValue::from_f32(1.5))
            .unwrap();
        assert_eq!(e, 1.5);

        let tuple = e.to_tuple().unwrap();
        let restored = f32::from_tuple(&tuple).unwrap();
        assert_eq!(restored, 1.5);
    }

    #[test]
    fn f64_entity_lifecycle() {
        let mut e: f64 = Entity::new_empty();
        assert_eq!(e, 0.0);

        assert_eq!(<f64 as Entity>::object_name(), "object_f64");

        e.set_field_value("field_f64", DataValue::from_f64(2.5))
            .unwrap();
        assert_eq!(e, 2.5);

        let tuple = e.to_tuple().unwrap();
        let restored = f64::from_tuple(&tuple).unwrap();
        assert_eq!(restored, 2.5);
    }
}
