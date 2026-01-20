#[cfg(test)]
mod tests {
    use crate::database::entity::Entity;
    use crate::database::test_object::object::Item;
    use mudu::utils::json;
    use mudu_type::dat_type::DatType;
    use mudu_type::datum::{Datum, DatumDyn};

    #[test]
    fn test() {
        let i = 0i64;
        println!("{}", json::to_json_str(&i).unwrap());
        let s = "string";
        println!("{}", json::to_json_str(&s).unwrap());
    }

    #[test]
    fn test_entity_to_object() {
        let mut item = Item::new_empty();
        item.set_i_id(1);
        item.set_i_name("name".to_string());
        item.set_i_im_id(30);
        item.set_i_data("data".to_string());
        item.set_i_price(40.0);
        let dt = Item::dat_type();
        let value = item.to_value(dt).unwrap();
        let tuple = item.to_tuple().unwrap();
        let item_from_tuple = Item::from_tuple(&tuple).unwrap();
        let item_from_value = Item::from_value(&value).unwrap();
        let id = dt.dat_type_id();

        let printable = id.fn_output()(&value, dt).unwrap();
        let value2 = id.fn_input()(&printable, dt).unwrap();
        let item_from_value2 = Item::from_value(&value2).unwrap();

        let binary = id.fn_send()(&value, dt).unwrap();
        let (value3, _) = id.fn_recv()(&binary, dt).unwrap();
        let item_from_value3 = Item::from_value(&value3).unwrap();

        for (i, s) in [
            (item_from_tuple, "Entity::to_tuple -> Entity::from_tuple"),
            (item_from_value, "Entity::to_value -> Entity::from_value"),
            (item_from_value2, "Entity::to_value -> Output Printable -> Input Printable -> Entity::from_value"),
            (item_from_value3, "Entity::to_value -> Send Binary -> Recv Binary -> Entity::from_value"),
        ].iter() {
            assert_eq!(i.get_i_id(), item.get_i_id(), "{}", s);
            assert_eq!(i.get_i_name(), item.get_i_name(), "{}", s);
            assert_eq!(i.get_i_im_id(), item.get_i_im_id(), "{}", s);
            assert_eq!(i.get_i_data(), item.get_i_data(), "{}", s);
            assert_eq!(i.get_i_price(), item.get_i_price(), "{}", s);
        }
    }


    #[test]
    fn test_vec_entity_to_array() {
        let mut items = Vec::new();
        for i in 0..10 {
            let mut item = Item::new_empty();
            item.set_i_id(i + 11);
            item.set_i_name("name".to_string());
            item.set_i_im_id(30);
            item.set_i_data("data".to_string());
            item.set_i_price(40.0);
            items.push(item);
        }
        let ty: DatType = <Vec<Item>>::dat_type().clone();
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
            assert_eq!(items_from_value[i].get_i_id().unwrap(), item.get_i_id().unwrap());
            assert_eq!(items_from_binary[i].get_i_id().unwrap(), item.get_i_id().unwrap());
            assert_eq!(items_from_textual[i].get_i_id().unwrap(), item.get_i_id().unwrap());
        }
    }
}