#[cfg(test)]
mod tests {
    use crate::data_type_impl::lang::utils::{
        lang_type_name_2_type_family, type_family_2_lang_type_name,
    };
    use crate::type_family::TypeFamily;

    #[test]
    fn type_family_2_lang_type_name_maps_unique_ids() {
        let id_name = vec![
            (TypeFamily::I32, "i32"),
            (TypeFamily::I64, "i64"),
            (TypeFamily::String, "String"),
        ];
        let map = type_family_2_lang_type_name(&id_name);
        assert_eq!(map.get(&TypeFamily::I32).unwrap(), "i32");
        assert_eq!(map.get(&TypeFamily::I64).unwrap(), "i64");
        assert_eq!(map.get(&TypeFamily::String).unwrap(), "String");
    }

    #[test]
    fn lang_type_name_2_type_family_maps_unique_names() {
        let id_name = vec![
            (TypeFamily::I32, "i32"),
            (TypeFamily::I64, "i64"),
            (TypeFamily::String, "String"),
        ];
        let map = lang_type_name_2_type_family(&id_name);
        assert_eq!(map.get("i32").unwrap().0, TypeFamily::I32);
        assert_eq!(map.get("i64").unwrap().0, TypeFamily::I64);
        assert_eq!(map.get("String").unwrap().0, TypeFamily::String);
    }

    #[test]
    fn lang_type_name_2_type_family_last_id_wins_on_duplicate_names() {
        let id_name = vec![(TypeFamily::I32, "shared"), (TypeFamily::I64, "shared")];
        let map = lang_type_name_2_type_family(&id_name);
        assert_eq!(map.get("shared").unwrap().0, TypeFamily::I64);
    }

    #[test]
    fn roundtrip_between_helpers() {
        let id_name = vec![
            (TypeFamily::I32, "i32"),
            (TypeFamily::I64, "i64"),
            (TypeFamily::F32, "f32"),
            (TypeFamily::F64, "f64"),
            (TypeFamily::String, "String"),
        ];
        let id2name = type_family_2_lang_type_name(&id_name);
        let name2id = lang_type_name_2_type_family(&id_name);

        for (id, name) in &id_name {
            assert_eq!(id2name.get(id).unwrap().as_str(), *name);
            assert_eq!(name2id.get(*name).unwrap().0, *id);
        }
    }
}
