#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::database::attr_value::AttrValue;
    use crate::database::entity::Entity;
    use crate::database::test_object::object::{
        AttrIData, AttrIId, AttrIImId, AttrIName, AttrIPrice, Item,
    };
    use mudu_type::data_value::DataValue;
    use mudu_type::datum::{Datum, DatumDyn};

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
    fn item_getters_and_setters() {
        let mut item = Item::new_empty();
        assert!(item.get_i_id().is_none());

        item.set_i_id(42);
        assert_eq!(*item.get_i_id(), Some(42));

        item.set_i_name("name".to_string());
        assert_eq!(item.get_i_name().as_ref().unwrap(), "name");

        item.set_i_price(1.5);
        assert_eq!(*item.get_i_price(), Some(1.5));

        item.set_i_data("data".to_string());
        assert_eq!(item.get_i_data().as_ref().unwrap(), "data");

        item.set_i_im_id(7);
        assert_eq!(*item.get_i_im_id(), Some(7));
    }

    #[test]
    fn item_entity_object_name() {
        assert_eq!(<Item as Entity>::object_name(), "item");
    }

    #[test]
    fn item_entity_field_binary_roundtrip() {
        let item = sample_item();
        let binary = item.get_field_binary("i_id").unwrap().unwrap();
        let mut restored = Item::new_empty();
        restored.set_field_binary("i_id", binary).unwrap();
        assert_eq!(restored.get_i_id(), item.get_i_id());
    }

    #[test]
    fn item_entity_field_value_roundtrip() {
        let item = sample_item();
        let value = item.get_field_value("i_name").unwrap().unwrap();
        let mut restored = Item::new_empty();
        restored.set_field_value("i_name", value).unwrap();
        assert_eq!(restored.get_i_name(), item.get_i_name());
    }

    #[test]
    fn item_datum_binary_roundtrip() {
        let item = sample_item();
        let binary = item.to_binary(&Item::data_type()).unwrap();
        let restored = Item::from_binary(binary.as_ref()).unwrap();
        assert_eq!(restored.get_i_id(), item.get_i_id());
    }

    #[test]
    fn item_datum_value_roundtrip() {
        let item = sample_item();
        let value = item.to_value(&Item::data_type()).unwrap();
        let restored = Item::from_value(&value).unwrap();
        assert_eq!(restored.get_i_id(), item.get_i_id());
    }

    #[test]
    fn item_datum_textual_roundtrip() {
        let item = sample_item();
        let textual = item.to_textual(&Item::data_type()).unwrap();
        let restored = Item::from_textual(textual.as_ref()).unwrap();
        assert_eq!(restored.get_i_id(), item.get_i_id());
    }

    #[test]
    fn item_datum_type_family_and_clone_boxed() {
        let item = sample_item();
        assert_eq!(
            item.type_family().unwrap(),
            mudu_type::type_family::TypeFamily::Record
        );

        let boxed = item.clone_boxed();
        let value = boxed.to_value(&Item::data_type()).unwrap();
        let restored = Item::from_value(&value).unwrap();
        assert_eq!(restored.get_i_id(), item.get_i_id());
    }

    #[test]
    fn item_attr_value_metadata() {
        assert_eq!(AttrIId::object_name(), "item");
        assert_eq!(AttrIId::attr_name(), "i_id");
        assert_eq!(
            AttrIId::data_type().type_family(),
            mudu_type::type_family::TypeFamily::I32
        );

        assert_eq!(AttrIName::object_name(), "item");
        assert_eq!(AttrIName::attr_name(), "i_name");
        assert_eq!(
            AttrIName::data_type().type_family(),
            mudu_type::type_family::TypeFamily::String
        );

        assert_eq!(AttrIPrice::object_name(), "item");
        assert_eq!(AttrIPrice::attr_name(), "i_price");
        assert_eq!(
            AttrIPrice::data_type().type_family(),
            mudu_type::type_family::TypeFamily::F64
        );

        assert_eq!(AttrIData::object_name(), "item");
        assert_eq!(AttrIData::attr_name(), "i_data");
        assert_eq!(
            AttrIData::data_type().type_family(),
            mudu_type::type_family::TypeFamily::String
        );

        assert_eq!(AttrIImId::object_name(), "item");
        assert_eq!(AttrIImId::attr_name(), "i_im_id");
        assert_eq!(
            AttrIImId::data_type().type_family(),
            mudu_type::type_family::TypeFamily::I32
        );
    }

    #[test]
    #[should_panic(expected = "unknown name")]
    fn item_get_field_binary_unknown_panics() {
        let item = sample_item();
        let _ = item.get_field_binary("unknown");
    }

    #[test]
    #[should_panic(expected = "unknown name")]
    fn item_set_field_binary_unknown_panics() {
        let mut item = Item::new_empty();
        let _ = item.set_field_binary("unknown", vec![]);
    }

    #[test]
    #[should_panic(expected = "unknown name")]
    fn item_get_field_value_unknown_panics() {
        let item = sample_item();
        let _ = item.get_field_value("unknown");
    }

    #[test]
    #[should_panic(expected = "unknown name")]
    fn item_set_field_value_unknown_panics() {
        let mut item = Item::new_empty();
        let _ = item.set_field_value("unknown", DataValue::from_i32(1));
    }
}
