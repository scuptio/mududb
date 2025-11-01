#[cfg(any(test, feature = "test"))]
use crate::data_type::dt_fn_arbitrary::FnArbitrary;
use crate::data_type::dt_fn_compare::FnCompare;
use crate::data_type::dt_fn_convert::FnBase;
use crate::data_type::dt_impl::dat_type_id::DatTypeID;

use crate::data_type::dt_impl;

use crate::data_type::dt_param::FnParam;
use crate::data_type::param_obj::ParamObj;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::string::ToString;

struct DatTypeDef {
    pub id: DatTypeID,
    /// data type ID
    pub type_name: String,
    /// data type name in SQL
    pub fn_base: FnBase,
    /// base function
    pub opt_fn_compare: Option<FnCompare>,
    /// optional compare function
    #[cfg(any(test, feature = "test"))]
    pub fn_arbitrary: FnArbitrary,
    /// arbitrary function
    pub opt_fn_param: Option<FnParam>,
    /// is fixed length
    pub is_fixed_len: bool,
}

lazy_static! {
    static ref DAT_TABLE: Vec<DatTypeDef> = vec![
        DatTypeDef {
            id: DatTypeID::I32,
            type_name: String::from("int"),
            fn_base: dt_impl::fn_i32::FN_I32_CONVERT,
            opt_fn_compare: Some(dt_impl::fn_i32::FN_I32_COMPARE),
            #[cfg(any(test, feature = "test"))]
            fn_arbitrary: dt_impl::fn_i32_arb::FN_I32_ARBITRARY,
            is_fixed_len: true,
            opt_fn_param: None,
        },
        DatTypeDef {
            id: DatTypeID::I64,
            type_name: String::from("bigint"),
            fn_base: dt_impl::fn_i64::FN_I64_CONVERT,
            opt_fn_compare: Some(dt_impl::fn_i64::FN_I64_COMPARE),
            #[cfg(any(test, feature = "test"))]
            fn_arbitrary: dt_impl::fn_i64_arb::FN_I64_ARBITRARY,
            is_fixed_len: true,
            opt_fn_param: None,
        },
        DatTypeDef {
            id: DatTypeID::F32,
            type_name: "float".to_string(),
            fn_base: dt_impl::fn_f32::FN_F32_CONVERT,
            opt_fn_compare: None,
            #[cfg(any(test, feature = "test"))]
            fn_arbitrary: dt_impl::fn_f32_arb::FN_F32_ARBITRARY,
            is_fixed_len: true,
            opt_fn_param: None,
        },
        DatTypeDef {
            id: DatTypeID::F64,
            type_name: "double".to_string(),
            fn_base: dt_impl::fn_f64::FN_F64_CONVERT,
            opt_fn_compare: None,
            #[cfg(any(test, feature = "test"))]
            fn_arbitrary: dt_impl::fn_f64_arb::FN_F64_ARBITRARY,
            is_fixed_len: true,
            opt_fn_param: None,
        },
        DatTypeDef {
            id: DatTypeID::CharFixedLen,
            type_name: "char".to_string(),
            fn_base: dt_impl::fn_char_fixed::FN_CHAR_FIXED_CONVERT,
            opt_fn_compare: Some(dt_impl::fn_char_fixed::FN_CHAR_FIXED_COMPARE),
            #[cfg(any(test, feature = "test"))]
            fn_arbitrary: dt_impl::fn_char_fixed_arb::FN_CHAR_FIXED_ARBITRARY,
            is_fixed_len: true,
            opt_fn_param: Some(dt_impl::fn_char_fixed_param::FN_CHAR_FIXED_PARAM),
        },
        DatTypeDef {
            id: DatTypeID::CharVarLen,
            type_name: "varchar".to_string(),
            fn_base: dt_impl::fn_char_var::FN_CHAR_VAR_CONVERT,
            opt_fn_compare: Some(dt_impl::fn_char_var::FN_CHAR_VAR_COMPARE),
            #[cfg(any(test, feature = "test"))]
            fn_arbitrary: dt_impl::fn_char_var_arb::FN_CHAR_VAR_ARBITRARY,
            is_fixed_len: false,
            opt_fn_param: Some(dt_impl::fn_char_var_param::FN_VARCHAR_PARAM),
        },
        // more data type definition
    ];

    static ref _DT_NAME_2_ID: HashMap<String, DatTypeID> = {
        let mut map: HashMap<String, DatTypeID> = HashMap::new();
        for _i in 0..DAT_TABLE.len() {
            let id = DAT_TABLE[_i].id;
            let name = DAT_TABLE[_i].type_name.clone();
            let _ = map.insert(name, id);
        }
        map
    };
}

pub fn dt_count() -> u32 {
    DAT_TABLE.len() as u32
}

pub fn get_dt_type(id: u32) -> DatTypeID {
    DAT_TABLE[id as usize].id
}

pub fn get_dt_name(id: u32) -> &'static str {
    DAT_TABLE[id as usize].type_name.as_str()
}

pub fn get_fn_convert(id: u32) -> &'static FnBase {
    &DAT_TABLE[id as usize].fn_base
}

pub fn get_opt_fn_compare(id: u32) -> &'static Option<FnCompare> {
    &DAT_TABLE[id as usize].opt_fn_compare
}

#[cfg(any(test, feature = "test"))]
pub fn get_fn_arbitrary(id: u32) -> &'static FnArbitrary {
    &DAT_TABLE[id as usize].fn_arbitrary
}

pub fn get_opt_fn_param(id: u32) -> &'static Option<FnParam> {
    &DAT_TABLE[id as usize].opt_fn_param
}

pub fn is_fixed_len(id: u32) -> bool {
    DAT_TABLE[id as usize].is_fixed_len
}

pub fn type_len(id: u32, opt_params: &ParamObj) -> Option<usize> {
    let type_def = &DAT_TABLE[id as usize];
    let fn_len = type_def.fn_base.len;
    fn_len(opt_params)
}

pub fn has_param(id: u32) -> bool {
    DAT_TABLE[id as usize].opt_fn_param.is_some()
}

pub fn get_fn_param(id: u32) -> Option<FnParam> {
    DAT_TABLE[id as usize].opt_fn_param.clone()
}

pub fn name_2_id(s: &str) -> DatTypeID {
    let opt = _DT_NAME_2_ID.get(&s.to_string());
    if let Some(_id) = opt {
        *_id
    } else {
        panic!("unknown data type {}", s);
    }
}
