use crate::data_type_impl::lang::utils::{
    lang_type_name_2_type_family, type_family_2_lang_type_name,
};
use crate::type_family::TypeFamily;
use lazy_static::lazy_static;
use std::collections::HashMap;

lazy_static! {
    static ref _id_lang_type_name: Vec<(TypeFamily, &'static str)> = vec![
        (TypeFamily::I32, "i32"),
        (TypeFamily::I64, "i64"),
        (TypeFamily::I128, "i128"),
        (TypeFamily::U128, "OID"),
        (TypeFamily::F32, "f32"),
        (TypeFamily::F64, "f64"),
        (TypeFamily::String, "String"),
        (TypeFamily::Array, "Vec"),
        (TypeFamily::Record, "Record"),
        (TypeFamily::Binary, "Vec<u8>")
    ];
    static ref _id2name: HashMap<TypeFamily, String> =
        type_family_2_lang_type_name(&_id_lang_type_name);
    static ref _name2id: HashMap<String, (TypeFamily, Vec<TypeFamily>)> = {
        let mut map = lang_type_name_2_type_family(&_id_lang_type_name);
        map.insert("u128".to_string(), (TypeFamily::U128, Default::default()));
        map
    };
}

pub fn dt_lang_name_to_id(name: &str) -> Option<(TypeFamily, Vec<TypeFamily>)> {
    _name2id.get(name).cloned()
}

pub fn dt_id_to_lang_name(id: TypeFamily) -> Option<String> {
    _id2name.get(&id).cloned()
}
