#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::database::entity::Entity;
    use crate::database::entity_utils;
    use crate::database::test_object::object::Item;
    use crate::tuple::tuple_field::TupleField;
    use crate::tuple::tuple_value::TupleValue;
    use mudu::error::ErrorCode;
    use mudu_type::data_type::DataType;
    use mudu_type::data_value::DataValue;
    use mudu_type::datum::Datum;
    use mudu_type::type_family::TypeFamily;

    fn sample_item() -> Item {
        let mut item = Item::new_empty();
        item.set_i_id(1);
        item.set_i_name("item_name".to_string());
        item.set_i_price(9.99);
        item.set_i_data("data".to_string());
        item.set_i_im_id(100);
        item
    }

    #[test]
    fn entity_from_tuple_field_roundtrip() {
        let tuple = i32::new_empty().to_tuple().unwrap();
        let value: i32 = entity_utils::entity_from_tuple_field(&tuple).unwrap();
        assert_eq!(value, 0);

        let mut e = i32::new_empty();
        e.set_field_value("field_i32", DataValue::from_i32(42))
            .unwrap();
        let tuple = e.to_tuple().unwrap();
        let restored: i32 = entity_utils::entity_from_tuple_field(&tuple).unwrap();
        assert_eq!(restored, 42);
    }

    #[test]
    fn entity_from_tuple_field_rejects_length_mismatch() {
        let tuple = TupleField::new(vec![vec![0, 0, 0, 1], vec![0, 0, 0, 2]]);
        let err = entity_utils::entity_from_tuple_field::<i32, _>(&tuple).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidType);
    }

    #[test]
    fn entity_from_tuple_field_rejects_null_field() {
        let tuple = TupleField::new_nullable(vec![None]);
        let err = entity_utils::entity_from_tuple_field::<i32, _>(&tuple).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    }

    #[test]
    fn entity_from_tuple_value_roundtrip() {
        let row = TupleValue::from(vec![DataValue::from_i32(42)]);
        let value: i32 = entity_utils::entity_from_tuple_value(&row).unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn entity_from_tuple_value_rejects_length_mismatch() {
        let row = TupleValue::from(vec![DataValue::from_i32(1), DataValue::from_i32(2)]);
        let err = entity_utils::entity_from_tuple_value::<i32, _>(&row).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidType);
    }

    #[test]
    fn entity_from_value_roundtrip_scalar() {
        let record = DataValue::from_record(vec![DataValue::from_i32(42)]);
        let entity: i32 = entity_utils::entity_from_value(&record).unwrap();
        assert_eq!(entity, 42);
    }

    #[test]
    fn entity_from_value_rejects_non_record() {
        let value = DataValue::from_i32(42);
        let err = entity_utils::entity_from_value::<Item, _>(&value).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidType);
    }

    #[test]
    fn entity_from_value_rejects_wrong_field_length() {
        let record = DataValue::from_record(vec![DataValue::from_i32(1)]);
        let err = entity_utils::entity_from_value::<Item, _>(&record).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidType);
    }

    #[test]
    fn entity_type_family_is_record() {
        assert_eq!(
            entity_utils::entity_type_family().unwrap(),
            TypeFamily::Record
        );
    }

    #[test]
    fn entity_data_type_for_scalar() {
        let ty = entity_utils::entity_data_type::<i32>();
        assert_eq!(ty.type_family(), TypeFamily::Record);
    }

    #[test]
    fn entity_data_type_for_item() {
        let ty = entity_utils::entity_data_type::<Item>();
        assert_eq!(ty.type_family(), TypeFamily::Record);
        let fields = ty.expect_record_param().fields();
        assert_eq!(fields.len(), 5);
    }

    #[test]
    fn entity_to_tuple_roundtrip() {
        let entity = sample_item();
        let tuple = entity_utils::entity_to_tuple(&entity).unwrap();
        let restored: Item = entity_utils::entity_from_tuple_field(&tuple).unwrap();
        assert_eq!(restored.get_i_id(), entity.get_i_id());
        assert_eq!(restored.get_i_name(), entity.get_i_name());
    }

    #[test]
    fn entity_to_tuple_rejects_none_field() {
        let item = Item::new_empty();
        let err = entity_utils::entity_to_tuple(&item).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidType);
    }

    #[test]
    fn entity_to_value_roundtrip() {
        let entity = sample_item();
        let ty = Item::data_type();
        let value = entity_utils::entity_to_value(&entity, &ty).unwrap();
        let restored: Item = entity_utils::entity_from_value(&value).unwrap();
        assert_eq!(restored.get_i_id(), entity.get_i_id());
    }

    #[test]
    fn entity_to_value_rejects_non_record_type() {
        let entity = sample_item();
        let ty = i32::data_type();
        let err = entity_utils::entity_to_value(&entity, &ty).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }

    #[test]
    fn entity_to_value_rejects_none_field() {
        let item = Item::new_empty();
        let ty = Item::data_type();
        let err = entity_utils::entity_to_value(&item, &ty).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidType);
    }

    #[test]
    fn entity_to_binary_roundtrip() {
        let entity = sample_item();
        let ty = Item::data_type();
        let binary = entity_utils::entity_to_binary(&entity, &ty).unwrap();
        let restored: Item = entity_utils::entity_from_binary(binary.as_ref()).unwrap();
        assert_eq!(restored.get_i_id(), entity.get_i_id());
    }

    #[test]
    fn entity_to_textual_roundtrip() {
        let entity = sample_item();
        let ty = Item::data_type();
        let textual = entity_utils::entity_to_textual(&entity, &ty).unwrap();
        let restored: Item = entity_utils::entity_from_textual(textual.as_ref()).unwrap();
        assert_eq!(restored.get_i_id(), entity.get_i_id());
    }

    #[test]
    fn entity_clone_boxed_roundtrip() {
        let entity = sample_item();
        let boxed = entity_utils::entity_clone_boxed(&entity);
        let cloned = boxed.as_ref().clone_boxed();
        let value = cloned.to_value(&Item::data_type()).unwrap();
        let restored: Item = entity_utils::entity_from_value(&value).unwrap();
        assert_eq!(restored.get_i_id(), entity.get_i_id());
    }

    #[test]
    fn entity_from_textual_rejects_invalid_input() {
        let err = entity_utils::entity_from_textual::<Item>("not a valid record").unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }

    #[test]
    fn entity_from_binary_rejects_invalid_input() {
        let err = entity_utils::entity_from_binary::<Item>(&[0xff; 64]).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }

    #[test]
    fn entity_to_binary_rejects_non_record_type() {
        let entity = sample_item();
        let ty = i32::data_type();
        let err = entity_utils::entity_to_binary(&entity, &ty).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }

    #[test]
    fn entity_to_textual_rejects_non_record_type() {
        let entity = sample_item();
        let ty = i32::data_type();
        let err = entity_utils::entity_to_textual(&entity, &ty).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }

    #[test]
    fn entity_to_binary_propagates_send_error() {
        let entity = 42i32;
        let ty = DataType::default_for(TypeFamily::Numeric);
        let err = entity_utils::entity_to_binary(&entity, &ty).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }

    #[test]
    fn entity_to_textual_propagates_output_error() {
        let entity = 42i32;
        let ty = DataType::default_for(TypeFamily::Numeric);
        let err = entity_utils::entity_to_textual(&entity, &ty).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }
}
