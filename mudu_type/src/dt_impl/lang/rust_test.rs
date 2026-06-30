#[cfg(test)]
mod tests {
    use crate::dat_type_id::DatTypeID;
    use crate::dt_impl::lang::rust::{dt_id_to_lang_name, dt_lang_name_to_id};

    #[test]
    fn dt_lang_name_to_id_known_types() {
        assert_eq!(dt_lang_name_to_id("i32").unwrap().0, DatTypeID::I32);
        assert_eq!(dt_lang_name_to_id("i64").unwrap().0, DatTypeID::I64);
        assert_eq!(dt_lang_name_to_id("i128").unwrap().0, DatTypeID::I128);
        assert_eq!(dt_lang_name_to_id("f32").unwrap().0, DatTypeID::F32);
        assert_eq!(dt_lang_name_to_id("f64").unwrap().0, DatTypeID::F64);
        assert_eq!(dt_lang_name_to_id("String").unwrap().0, DatTypeID::String);
        assert_eq!(dt_lang_name_to_id("Vec").unwrap().0, DatTypeID::Array);
        assert_eq!(dt_lang_name_to_id("Record").unwrap().0, DatTypeID::Record);
        assert_eq!(dt_lang_name_to_id("Vec<u8>").unwrap().0, DatTypeID::Binary);
        assert_eq!(dt_lang_name_to_id("OID").unwrap().0, DatTypeID::U128);
        assert_eq!(dt_lang_name_to_id("u128").unwrap().0, DatTypeID::U128);
        assert!(dt_lang_name_to_id("unknown").is_none());
    }

    #[test]
    fn dt_id_to_lang_name_known_types() {
        assert_eq!(dt_id_to_lang_name(DatTypeID::I32).unwrap(), "i32");
        assert_eq!(dt_id_to_lang_name(DatTypeID::I64).unwrap(), "i64");
        assert_eq!(dt_id_to_lang_name(DatTypeID::I128).unwrap(), "i128");
        assert_eq!(dt_id_to_lang_name(DatTypeID::U128).unwrap(), "OID");
        assert_eq!(dt_id_to_lang_name(DatTypeID::F32).unwrap(), "f32");
        assert_eq!(dt_id_to_lang_name(DatTypeID::F64).unwrap(), "f64");
        assert_eq!(dt_id_to_lang_name(DatTypeID::String).unwrap(), "String");
        assert_eq!(dt_id_to_lang_name(DatTypeID::Array).unwrap(), "Vec");
        assert_eq!(dt_id_to_lang_name(DatTypeID::Record).unwrap(), "Record");
        assert_eq!(dt_id_to_lang_name(DatTypeID::Binary).unwrap(), "Vec<u8>");
    }
}
