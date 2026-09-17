#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::database::entity::Entity;
    use crate::database::field_change::FieldChange;
    use crate::database::test_object::object::{Item, ItemChange, columns};
    use mudu_type::datum::{Datum, DatumDyn};
    use mudu_type::type_family::TypeFamily;

    fn sample_item() -> Item {
        Item::new(
            1,
            Some("item_name".to_string()),
            9.99,
            Some("data".to_string()),
            Some(100),
        )
    }

    #[test]
    fn item_struct_field_access() {
        let item = sample_item();
        assert_eq!(item.i_id, 1);
        assert_eq!(item.i_name.as_deref(), Some("item_name"));
        assert_eq!(item.i_price, 9.99);
        assert_eq!(item.i_data.as_deref(), Some("data"));
        assert_eq!(item.i_im_id, Some(100));
    }

    #[test]
    fn item_insert_params() {
        let item = sample_item();
        let params = item.insert_params();
        assert_eq!(params.0, 1);
        assert_eq!(params.1.as_deref(), Some("item_name"));
        assert_eq!(params.2, 9.99);
        assert_eq!(params.3.as_deref(), Some("data"));
        assert_eq!(params.4, Some(100));
    }

    #[test]
    fn item_update_set_clause() {
        let update = ItemChange {
            i_name: FieldChange::Set(Some("new_name".to_string())),
            i_price: FieldChange::Set(1.5),
            ..ItemChange::default()
        };
        assert!(!update.is_empty());
        let (sql, params) = update.update_by_pk(42).unwrap();
        assert_eq!(
            sql,
            format!(
                "UPDATE item SET {} = ?, {} = ? WHERE {} = ?",
                columns::I_NAME,
                columns::I_PRICE,
                columns::I_ID
            )
        );
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn item_update_set_none_writes_null() {
        let update = ItemChange {
            i_name: FieldChange::Set(None),
            ..ItemChange::default()
        };
        assert!(!update.is_empty());
        let (sql, params) = update.update_by_pk(42).unwrap();
        assert_eq!(
            sql,
            format!(
                "UPDATE item SET {} = ? WHERE {} = ?",
                columns::I_NAME,
                columns::I_ID
            )
        );
        assert_eq!(params.len(), 2);
        let null_value = params[0].to_value(&String::data_type()).unwrap();
        assert!(null_value.is_null());
    }

    #[test]
    fn item_update_empty_yields_no_statement() {
        let update = ItemChange::default();
        assert!(update.is_empty());
        assert!(update.update_by_pk(42).is_none());
    }

    #[test]
    fn item_entity_metadata() {
        assert_eq!(<Item as Entity>::table_name(), "item");
        assert_eq!(Item::TABLE_NAME, "item");
        assert_eq!(
            Item::SQL_GET_BY_PK,
            "SELECT i_id, i_name, i_price, i_data, i_im_id FROM item WHERE i_id = ?"
        );
        assert!(Item::SQL_INSERT.starts_with("INSERT INTO item"));
        assert_eq!(Item::SQL_DELETE_BY_PK, "DELETE FROM item WHERE i_id = ?");
        let desc = <Item as Entity>::tuple_desc();
        assert_eq!(desc.fields().len(), 5);
        assert_eq!(desc.fields()[0].name(), columns::I_ID);
        assert_eq!(desc.fields()[2].name(), columns::I_PRICE);
        assert_eq!(desc.fields()[2].data_type().type_family(), TypeFamily::F64);
    }

    #[test]
    fn item_tuple_roundtrip_with_nulls() {
        let mut item = sample_item();
        item.i_data = None;
        let tuple = item.to_tuple().unwrap();
        assert!(tuple.is_null(3));
        let restored = Item::from_tuple(&tuple).unwrap();
        assert_eq!(restored.i_id, item.i_id);
        assert_eq!(restored.i_name, item.i_name);
        assert_eq!(restored.i_price, item.i_price);
        assert_eq!(restored.i_data, None);
        assert_eq!(restored.i_im_id, item.i_im_id);
    }

    #[test]
    fn item_from_tuple_rejects_null_not_null_column() {
        let mut item = sample_item();
        item.i_price = 1.0;
        let mut tuple = item.to_tuple().unwrap();
        tuple.mut_fields()[2] = None;
        let err = Item::from_tuple(&tuple).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidType);
    }

    #[test]
    fn item_tuple_value_roundtrip_with_nulls() {
        let item = Item::new(7, None, 3.5, None, None);
        let value = item.to_tuple_value().unwrap();
        assert!(value.values()[1].is_null());
        assert!(value.values()[3].is_null());
        assert!(value.values()[4].is_null());
        let restored = Item::from_tuple_value(&value).unwrap();
        assert_eq!(restored.i_id, 7);
        assert_eq!(restored.i_name, None);
        assert_eq!(restored.i_price, 3.5);
    }

    #[test]
    fn item_datum_binary_roundtrip() {
        let item = sample_item();
        let binary = item.to_binary(&Item::data_type()).unwrap();
        let restored = Item::from_binary(binary.as_ref()).unwrap();
        assert_eq!(restored.i_id, item.i_id);
    }

    #[test]
    fn item_datum_value_roundtrip() {
        let item = sample_item();
        let value = item.to_value(&Item::data_type()).unwrap();
        let restored = Item::from_value(&value).unwrap();
        assert_eq!(restored.i_id, item.i_id);
    }

    #[test]
    fn item_datum_textual_roundtrip() {
        let item = sample_item();
        let textual = item.to_textual(&Item::data_type()).unwrap();
        let restored = Item::from_textual(textual.as_ref()).unwrap();
        assert_eq!(restored.i_id, item.i_id);
    }

    #[test]
    fn item_datum_type_family_and_clone_boxed() {
        let item = sample_item();
        assert_eq!(item.type_family().unwrap(), TypeFamily::Record);

        let boxed = item.clone_boxed();
        let value = boxed.to_value(&Item::data_type()).unwrap();
        let restored = Item::from_value(&value).unwrap();
        assert_eq!(restored.i_id, item.i_id);
    }
}
