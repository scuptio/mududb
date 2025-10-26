use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::dt_impl::lang::utils::{
    dat_type_id_2_lang_type_name, lang_type_name_2_dat_type_id,
};
use lazy_static::lazy_static;
use std::collections::HashMap;

lazy_static! {
    static ref _id_lang_type_name: Vec<(DatTypeID, &'static str)> = vec![
        (DatTypeID::I32, "i32"),
        (DatTypeID::I64, "i64"),
        (DatTypeID::F32, "f32"),
        (DatTypeID::F64, "f64"),
        (DatTypeID::CharFixedLen, "String"),
        (DatTypeID::CharVarLen, "String"),
    ];
    static ref _id2name: Vec<String> = dat_type_id_2_lang_type_name(&_id_lang_type_name);
    static ref _name2id: HashMap<String, (DatTypeID, Vec<DatTypeID>)> =
        lang_type_name_2_dat_type_id(&_id_lang_type_name);
}

pub fn dt_lang_name_to_id(name: &str) -> Option<(DatTypeID, Vec<DatTypeID>)> {
    _name2id.get(name).cloned()
}

pub fn dt_id_to_lang_name(id: DatTypeID) -> String {
    _id2name[id.to_u32() as usize].clone()
}
