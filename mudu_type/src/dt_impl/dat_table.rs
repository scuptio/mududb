use crate::dat_type_id::DatTypeID;
#[cfg(any(test, feature = "test"))]
use crate::dt_fn_arbitrary::FnArbitrary;
use crate::dt_fn_compare::FnCompare;
use crate::dt_fn_convert::FnBase;
use crate::dt_fn_param::FnParam;
use crate::dt_impl;
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
    /// is fixed length for all its subtype
    pub fixed_length: Option<u32>,
}

lazy_static! {
    static ref DAT_TYPE_DEF_TABLE: Vec<DatTypeDef> = vec![
        DatTypeDef {
            id: DatTypeID::I32,
            type_name: String::from("int"),
            fn_base: dt_impl::fn_i32::FN_I32_CONVERT,
            opt_fn_compare: Some(dt_impl::fn_i32::FN_I32_COMPARE),
            #[cfg(any(test, feature = "test"))]
            fn_arbitrary: dt_impl::fn_i32_arb::FN_I32_ARBITRARY,
            fixed_length: Some(size_of::<i32>() as u32),
            opt_fn_param: None,
        },
        DatTypeDef {
            id: DatTypeID::I64,
            type_name: String::from("bigint"),
            fn_base: dt_impl::fn_i64::FN_I64_CONVERT,
            opt_fn_compare: Some(dt_impl::fn_i64::FN_I64_COMPARE),
            #[cfg(any(test, feature = "test"))]
            fn_arbitrary: dt_impl::fn_i64_arb::FN_I64_ARBITRARY,
            fixed_length: Some(size_of::<i64>() as u32),
            opt_fn_param: None,
        },
        DatTypeDef {
            id: DatTypeID::F32,
            type_name: "float".to_string(),
            fn_base: dt_impl::fn_f32::FN_F32_CONVERT,
            opt_fn_compare: None,
            #[cfg(any(test, feature = "test"))]
            fn_arbitrary: dt_impl::fn_f32_arb::FN_F32_ARBITRARY,
            fixed_length: Some(size_of::<f32>() as u32),
            opt_fn_param: None,
        },
        DatTypeDef {
            id: DatTypeID::F64,
            type_name: "double".to_string(),
            fn_base: dt_impl::fn_f64::FN_F64_CONVERT,
            opt_fn_compare: None,
            #[cfg(any(test, feature = "test"))]
            fn_arbitrary: dt_impl::fn_f64_arb::FN_F64_ARBITRARY,
            fixed_length: Some(size_of::<f64>() as u32),
            opt_fn_param: None,
        },
        DatTypeDef {
            id: DatTypeID::String,
            type_name: "varchar".to_string(),
            fn_base: dt_impl::fn_string::FN_CHAR_FIXED_CONVERT,
            opt_fn_compare: Some(dt_impl::fn_string::FN_CHAR_FIXED_COMPARE),
            #[cfg(any(test, feature = "test"))]
            fn_arbitrary: dt_impl::fn_string_arb::FN_CHAR_FIXED_ARBITRARY,
            fixed_length: None,
            opt_fn_param: Some(dt_impl::fn_string_param::FN_CHAR_FIXED_PARAM),
        },
        DatTypeDef {
            id: DatTypeID::Array,
            type_name: "array".to_string(),
            fn_base: dt_impl::fn_array::FN_ARRAY_CONVERT,
            opt_fn_compare: None,
            #[cfg(any(test, feature = "test"))]
            fn_arbitrary: dt_impl::fn_array_arb::FN_ARRAY_ARBITRARY,
            fixed_length: None,
            opt_fn_param: Some(dt_impl::fn_array_param::FN_ARRAY_PARAM),
        },
        DatTypeDef {
            id: DatTypeID::Record,
            type_name: "object".to_string(),
            fn_base: dt_impl::fn_object::FN_OBJECT_CONVERT,
            opt_fn_compare: None,
            #[cfg(any(test, feature = "test"))]
            fn_arbitrary: dt_impl::fn_object_arb::FN_OBJECT_ARBITRARY,
            fixed_length: None,
            opt_fn_param: Some(dt_impl::fn_object_param::FN_OBJECT_PARAM),
        },
        DatTypeDef {
            id: DatTypeID::Binary,
            type_name: "binary".to_string(),
            fn_base: dt_impl::fn_binary::FN_BINARY_CONVERT,
            opt_fn_compare: None,
            #[cfg(any(test, feature = "test"))]
            fn_arbitrary: dt_impl::fn_binary_arb::FN_BINARY_ARBITRARY,
            fixed_length: None,
            opt_fn_param: None,
        },
        // more data type definition
    ];

    static ref _DT_NAME_2_ID: HashMap<String, DatTypeID> = {
        let mut map: HashMap<String, DatTypeID> = HashMap::new();
        for _i in 0..DAT_TYPE_DEF_TABLE.len() {
            let id = DAT_TYPE_DEF_TABLE[_i].id;
            let name = DAT_TYPE_DEF_TABLE[_i].type_name.clone();
            let _ = map.insert(name, id);
        }
        map
    };

    static ref DAT_TYPE_DEF_REF_TABLE: Vec<Option<&'static DatTypeDef>> = {
        let len = DatTypeID::max() as usize;
        let mut vec = Vec::with_capacity(len + 1);
        vec.resize(len + 1, None);
        for _i in 0..DAT_TYPE_DEF_TABLE.len() {
            let def =  &DAT_TYPE_DEF_TABLE[_i];
            vec[def.id.to_u32() as usize] = Some(def);
        };
        vec
    };
}

pub fn get_dt_name(id: u32) -> &'static str {
    get_dat_def(id).type_name.as_str()
}

pub fn get_fn_convert(id: u32) -> &'static FnBase {
    &get_dat_def(id).fn_base
}

pub fn get_opt_fn_compare(id: u32) -> &'static Option<FnCompare> {
    &get_dat_def(id).opt_fn_compare
}

#[cfg(any(test, feature = "test"))]
pub fn get_fn_arbitrary(id: u32) -> &'static FnArbitrary {
    &get_dat_def(id).fn_arbitrary
}

pub fn get_opt_fn_param(id: u32) -> &'static Option<FnParam> {
    &get_dat_def(id).opt_fn_param
}

pub fn is_all_fixed_len(id: u32) -> bool {
    get_dat_def(id).fixed_length.is_some()
}

pub fn get_fn_param(id: u32) -> Option<FnParam> {
    get_dat_def(id).opt_fn_param.clone()
}

fn get_dat_def(id: u32) -> &'static DatTypeDef {
    unsafe { DAT_TYPE_DEF_REF_TABLE[id as usize].unwrap_unchecked() }
}
