#[cfg(test)]
mod tests {
    use crate::data_type_impl::lang::rust::{dt_id_to_lang_name, dt_lang_name_to_id};
    use crate::type_family::TypeFamily;

    #[test]
    fn dt_lang_name_to_id_known_types() {
        assert_eq!(dt_lang_name_to_id("i32").unwrap().0, TypeFamily::I32);
        assert_eq!(dt_lang_name_to_id("i64").unwrap().0, TypeFamily::I64);
        assert_eq!(dt_lang_name_to_id("i128").unwrap().0, TypeFamily::I128);
        assert_eq!(dt_lang_name_to_id("f32").unwrap().0, TypeFamily::F32);
        assert_eq!(dt_lang_name_to_id("f64").unwrap().0, TypeFamily::F64);
        assert_eq!(dt_lang_name_to_id("String").unwrap().0, TypeFamily::String);
        assert_eq!(dt_lang_name_to_id("Vec").unwrap().0, TypeFamily::Array);
        assert_eq!(dt_lang_name_to_id("Record").unwrap().0, TypeFamily::Record);
        assert_eq!(dt_lang_name_to_id("Vec<u8>").unwrap().0, TypeFamily::Binary);
        assert_eq!(dt_lang_name_to_id("OID").unwrap().0, TypeFamily::U128);
        assert_eq!(dt_lang_name_to_id("u128").unwrap().0, TypeFamily::U128);
        assert!(dt_lang_name_to_id("unknown").is_none());
    }

    #[test]
    fn dt_id_to_lang_name_known_types() {
        assert_eq!(dt_id_to_lang_name(TypeFamily::I32).unwrap(), "i32");
        assert_eq!(dt_id_to_lang_name(TypeFamily::I64).unwrap(), "i64");
        assert_eq!(dt_id_to_lang_name(TypeFamily::I128).unwrap(), "i128");
        assert_eq!(dt_id_to_lang_name(TypeFamily::U128).unwrap(), "OID");
        assert_eq!(dt_id_to_lang_name(TypeFamily::F32).unwrap(), "f32");
        assert_eq!(dt_id_to_lang_name(TypeFamily::F64).unwrap(), "f64");
        assert_eq!(dt_id_to_lang_name(TypeFamily::String).unwrap(), "String");
        assert_eq!(dt_id_to_lang_name(TypeFamily::Array).unwrap(), "Vec");
        assert_eq!(dt_id_to_lang_name(TypeFamily::Record).unwrap(), "Record");
        assert_eq!(dt_id_to_lang_name(TypeFamily::Binary).unwrap(), "Vec<u8>");
    }
}
