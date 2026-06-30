#[cfg(test)]
mod tests {
    use crate::dat_type_id::DatTypeID;
    use crate::dt_impl::lang::utils::{dat_type_id_2_lang_type_name, lang_type_name_2_dat_type_id};

    #[test]
    fn dat_type_id_2_lang_type_name_maps_unique_ids() {
        let id_name = vec![
            (DatTypeID::I32, "i32"),
            (DatTypeID::I64, "i64"),
            (DatTypeID::String, "String"),
        ];
        let map = dat_type_id_2_lang_type_name(&id_name);
        assert_eq!(map.get(&DatTypeID::I32).unwrap(), "i32");
        assert_eq!(map.get(&DatTypeID::I64).unwrap(), "i64");
        assert_eq!(map.get(&DatTypeID::String).unwrap(), "String");
    }

    #[test]
    fn lang_type_name_2_dat_type_id_maps_unique_names() {
        let id_name = vec![
            (DatTypeID::I32, "i32"),
            (DatTypeID::I64, "i64"),
            (DatTypeID::String, "String"),
        ];
        let map = lang_type_name_2_dat_type_id(&id_name);
        assert_eq!(map.get("i32").unwrap().0, DatTypeID::I32);
        assert_eq!(map.get("i64").unwrap().0, DatTypeID::I64);
        assert_eq!(map.get("String").unwrap().0, DatTypeID::String);
    }

    #[test]
    fn lang_type_name_2_dat_type_id_last_id_wins_on_duplicate_names() {
        let id_name = vec![(DatTypeID::I32, "shared"), (DatTypeID::I64, "shared")];
        let map = lang_type_name_2_dat_type_id(&id_name);
        assert_eq!(map.get("shared").unwrap().0, DatTypeID::I64);
    }

    #[test]
    fn roundtrip_between_helpers() {
        let id_name = vec![
            (DatTypeID::I32, "i32"),
            (DatTypeID::I64, "i64"),
            (DatTypeID::F32, "f32"),
            (DatTypeID::F64, "f64"),
            (DatTypeID::String, "String"),
        ];
        let id2name = dat_type_id_2_lang_type_name(&id_name);
        let name2id = lang_type_name_2_dat_type_id(&id_name);

        for (id, name) in &id_name {
            assert_eq!(id2name.get(id).unwrap().as_str(), *name);
            assert_eq!(name2id.get(*name).unwrap().0, *id);
        }
    }
}
