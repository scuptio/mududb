//! Tests for object-to-value/binary/textual round trips.

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::database::object::Object;
    use crate::database::test_object::object::Item;
    use mudu::utils::json;
    use mudu_type::data_type::DataType;
    use mudu_type::datum::{Datum, DatumDyn};
    use mudu_type::type_family_ext::TypeFamilyExt;

    #[test]
    fn test() {
        let i = 0i64;
        println!("{}", json::to_json_str(&i).unwrap());
        let s = "string";
        println!("{}", json::to_json_str(&s).unwrap());
    }

    fn sample_item() -> Item {
        Item::new(
            1,
            Some("name".to_string()),
            40.0,
            Some("data".to_string()),
            Some(30),
        )
    }

    #[test]
    fn object_conversion_rejects_invalid_types_and_corrupt_encodings() {
        use crate::database::object::*;
        use mudu::error::ErrorCode;
        use mudu_type::data_value::DataValue;
        use mudu_type::type_family::TypeFamily;

        assert_eq!(
            object_from_value::<Item>(&DataValue::from_i32(1))
                .unwrap_err()
                .ec(),
            ErrorCode::InvalidType
        );
        assert_eq!(
            object_from_binary::<Item>(&[]).unwrap_err().ec(),
            ErrorCode::TypeConversionFailed
        );
        assert_eq!(
            object_from_textual::<Item>("not a record")
                .unwrap_err()
                .ec(),
            ErrorCode::TypeConversionFailed
        );
        let wrong_type = DataType::new_no_param(TypeFamily::I32);
        let item = sample_item();
        assert_eq!(
            object_to_value(&item, &wrong_type).unwrap_err().ec(),
            ErrorCode::TypeConversionFailed
        );
        assert_eq!(
            expect_field_count(1, 5).unwrap_err().ec(),
            ErrorCode::InvalidType
        );
        assert!(expect_field_count(5, 5).is_ok());

        let error = field_from_tuple_binary::<i32>("items", "id", &None).unwrap_err();
        assert_eq!(error.ec(), ErrorCode::InvalidType);
        assert!(error.message().contains("items.id"));
        assert_eq!(
            field_from_tuple_value::<i32>("items", "id", &DataValue::null())
                .unwrap_err()
                .ec(),
            ErrorCode::InvalidType
        );
        assert!(opt_field_from_tuple_binary::<i32>(&Some(vec![])).is_err());
        assert_eq!(
            opt_field_from_tuple_value::<i32>(&DataValue::null()).unwrap(),
            None
        );
        assert_eq!(
            opt_field_from_tuple_value::<i32>(&DataValue::from_i32(42)).unwrap(),
            Some(42)
        );
    }

    fn assert_same_item(lhs: &Item, rhs: &Item, context: &str) {
        assert_eq!(lhs.i_id, rhs.i_id, "{}", context);
        assert_eq!(lhs.i_name, rhs.i_name, "{}", context);
        assert_eq!(lhs.i_price, rhs.i_price, "{}", context);
        assert_eq!(lhs.i_data, rhs.i_data, "{}", context);
        assert_eq!(lhs.i_im_id, rhs.i_im_id, "{}", context);
    }

    #[test]
    fn test_object_to_object() {
        let item = sample_item();
        let dt = Item::data_type();
        let value = item.to_value(&dt).unwrap();
        let tuple = item.to_tuple().unwrap();
        let item_from_tuple = Item::from_tuple(&tuple).unwrap();
        let item_from_value = Item::from_value(&value).unwrap();
        let id = dt.type_family();

        let printable = id.fn_output()(&value, &dt).unwrap();
        let value2 = id.fn_input()(&printable, &dt).unwrap();
        let item_from_value2 = Item::from_value(&value2).unwrap();

        let binary = id.fn_send()(&value, &dt).unwrap();
        let (value3, _) = id.fn_recv()(&binary, &dt).unwrap();
        let item_from_value3 = Item::from_value(&value3).unwrap();

        for (i, s) in [
            (item_from_tuple, "Object::to_tuple -> Object::from_tuple"),
            (item_from_value, "Object::to_value -> Object::from_value"),
            (
                item_from_value2,
                "Object::to_value -> Output Printable -> Input Printable -> Object::from_value",
            ),
            (
                item_from_value3,
                "Object::to_value -> Send Binary -> Recv Binary -> Object::from_value",
            ),
        ]
        .iter()
        {
            assert_same_item(i, &item, s);
        }
    }

    #[test]
    fn test_vec_object_to_array() {
        let mut items = Vec::new();
        for i in 0..10 {
            let mut item = sample_item();
            item.i_id = i + 11;
            items.push(item);
        }
        let ty: DataType = <Vec<Item>>::data_type().clone();
        let value = items.to_value(&ty).unwrap();
        let binary = items.to_binary(&ty).unwrap().into();
        let textual = items.to_textual(&ty).unwrap().into();

        let items_from_value = <Vec<Item>>::from_value(&value).unwrap();
        let items_from_binary = <Vec<Item>>::from_binary(&binary).unwrap();
        let items_from_textual = <Vec<Item>>::from_textual(&textual).unwrap();
        assert_eq!(items_from_value.len(), items.len());
        assert_eq!(items_from_binary.len(), items.len());
        assert_eq!(items_from_textual.len(), items.len());
        for (i, item) in items.iter().enumerate() {
            assert_eq!(items_from_value[i].i_id, item.i_id);
            assert_eq!(items_from_binary[i].i_id, item.i_id);
            assert_eq!(items_from_textual[i].i_id, item.i_id);
        }
    }
}
