use crate::data_type::dt_fn_arbitrary::FnArbitrary;
use crate::data_type::dt_fn_base::FnBase;
use crate::data_type::dt_fn_compare::FnCompare;
use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::dt_impl::fn_char::{
    FN_FIXED_LEN_STRING_COMPARE, FN_FIXED_LEN_STRING_CONVERT,
};
use crate::data_type::dt_impl::fn_char_arb::FN_CHAR_ARBITRARY;
use crate::data_type::dt_impl::fn_char_param::FN_CHAR_PARAM;
use crate::data_type::dt_impl::fn_f32::FN_F32_CONVERT;
use crate::data_type::dt_impl::fn_f32_arb::FN_F32_ARBITRARY;
use crate::data_type::dt_impl::fn_f64::FN_F64_CONVERT;
use crate::data_type::dt_impl::fn_f64_arb::FN_F64_ARBITRARY;
use crate::data_type::dt_impl::fn_i32::{FN_I32_COMPARE, FN_I32_CONVERT};
use crate::data_type::dt_impl::fn_i32_arb::FN_I32_ARBITRARY;
use crate::data_type::dt_impl::fn_i64::{FN_I64_COMPARE, FN_I64_CONVERT};
use crate::data_type::dt_impl::fn_i64_arb::FN_I64_ARBITRARY;
use crate::data_type::dt_impl::fn_varchar::{FN_VAR_LEN_STRING_COMPARE, FN_VAR_LEN_STRING_CONVERT};
use crate::data_type::dt_impl::fn_varchar_arb::FN_VARCHAR_ARBITRARY;
use crate::data_type::dt_param::{FnParam, ParamObj};
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::string::ToString;

struct DataTypeDef {
    pub data_type_id: DatTypeID,
    pub data_type_name: String,
    pub fn_base: FnBase,
    pub opt_fn_compare: Option<FnCompare>,
    pub fn_arbitrary: FnArbitrary,
    pub is_fixed_length_type: bool,
    pub opt_fn_param: Option<FnParam>,
}

lazy_static! {
    static ref DAT_TABLE: Vec<DataTypeDef> = vec![
        DataTypeDef {
            data_type_id: DatTypeID::I32,
            data_type_name: String::from("int"),
            fn_base: FN_I32_CONVERT,
            opt_fn_compare: Some(FN_I32_COMPARE),
            fn_arbitrary: FN_I32_ARBITRARY,
            is_fixed_length_type: true,

            opt_fn_param: None,
        },
        DataTypeDef {
            data_type_id: DatTypeID::I64,
            data_type_name: String::from("bigint"),
            fn_base: FN_I64_CONVERT,
            opt_fn_compare: Some(FN_I64_COMPARE),
            fn_arbitrary: FN_I64_ARBITRARY,
            is_fixed_length_type: true,

            opt_fn_param: None,
        },
        DataTypeDef {
            data_type_id: DatTypeID::F32,
            data_type_name: "float".to_string(),
            fn_base: FN_F32_CONVERT,
            opt_fn_compare: None,
            fn_arbitrary: FN_F32_ARBITRARY,
            is_fixed_length_type: true,

            opt_fn_param: None,
        },
        DataTypeDef {
            data_type_id: DatTypeID::F64,
            data_type_name: "double".to_string(),
            fn_base: FN_F64_CONVERT,
            opt_fn_compare: None,
            fn_arbitrary: FN_F64_ARBITRARY,
            is_fixed_length_type: true,

            opt_fn_param: None,
        },
        DataTypeDef {
            data_type_id: DatTypeID::FixedLenString,
            data_type_name: "char".to_string(),
            fn_base: FN_FIXED_LEN_STRING_CONVERT,
            opt_fn_compare: Some(FN_FIXED_LEN_STRING_COMPARE),
            fn_arbitrary: FN_CHAR_ARBITRARY,
            is_fixed_length_type: true,

            opt_fn_param: Some(FN_CHAR_PARAM),
        },
        DataTypeDef {
            data_type_id: DatTypeID::VarLenString,
            data_type_name: "varchar".to_string(),
            fn_base: FN_VAR_LEN_STRING_CONVERT,
            opt_fn_compare: Some(FN_VAR_LEN_STRING_COMPARE),
            fn_arbitrary: FN_VARCHAR_ARBITRARY,
            is_fixed_length_type: false,

            opt_fn_param: Some(FN_CHAR_PARAM),
        },
    ];
    static ref _DT_NAME_2_ID: HashMap<String, DatTypeID> = {
        let mut map: HashMap<String, DatTypeID> = HashMap::new();
        for _i in 0..DAT_TABLE.len() {
            let id = DAT_TABLE[_i].data_type_id;
            let name = DAT_TABLE[_i].data_type_name.clone();
            let _ = map.insert(name, id);
        }
        map
    };
}

pub fn dt_count() -> u32 {
    DAT_TABLE.len() as u32
}

pub fn get_dt_type(id: u32) -> DatTypeID {
    DAT_TABLE[id as usize].data_type_id
}

pub fn get_dt_name(id: u32) -> &'static str {
    DAT_TABLE[id as usize].data_type_name.as_str()
}

pub fn get_convert_function(id: u32) -> &'static FnBase {
    &DAT_TABLE[id as usize].fn_base
}

pub fn get_compare_function(id: u32) -> &'static Option<FnCompare> {
    &DAT_TABLE[id as usize].opt_fn_compare
}

pub fn get_arbitrary_function(id: u32) -> &'static FnArbitrary {
    &DAT_TABLE[id as usize].fn_arbitrary
}

pub fn is_fixed_len(id: u32) -> bool {
    DAT_TABLE[id as usize].is_fixed_length_type
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
