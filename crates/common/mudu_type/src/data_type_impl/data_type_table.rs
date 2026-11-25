#[cfg(any(test, feature = "arbitrary"))]
use crate::data_type_fn_arbitrary::FnArbitrary;
use crate::data_type_fn_compare::FnCompare;
use crate::data_type_fn_convert::FnBase;
use crate::data_type_fn_param::FnParam;
use crate::data_type_impl;
use crate::type_family::TypeFamily;
use lazy_static::lazy_static;

struct DatTypeDef {
    pub id: TypeFamily,
    /// data type ID
    pub fn_base: FnBase,
    /// base function
    pub opt_fn_compare: Option<FnCompare>,
    /// optional compare function
    #[cfg(any(test, feature = "arbitrary"))]
    pub fn_arbitrary: FnArbitrary,
    /// arbitrary function
    pub opt_fn_param: Option<FnParam>,
}

lazy_static! {
    static ref DAT_TYPE_DEF_TABLE: Vec<DatTypeDef> = vec![
        DatTypeDef {
            id: TypeFamily::I32,
            fn_base: data_type_impl::fn_i32::FN_I32_CONVERT,
            opt_fn_compare: Some(data_type_impl::fn_i32::FN_I32_COMPARE),
            #[cfg(any(test, feature = "arbitrary"))]
            fn_arbitrary: data_type_impl::fn_i32_arb::FN_I32_ARBITRARY,
            opt_fn_param: None,
        },
        DatTypeDef {
            id: TypeFamily::I64,
            fn_base: data_type_impl::fn_i64::FN_I64_CONVERT,
            opt_fn_compare: Some(data_type_impl::fn_i64::FN_I64_COMPARE),
            #[cfg(any(test, feature = "arbitrary"))]
            fn_arbitrary: data_type_impl::fn_i64_arb::FN_I64_ARBITRARY,
            opt_fn_param: None,
        },
        DatTypeDef {
            id: TypeFamily::F32,
            fn_base: data_type_impl::fn_f32::FN_F32_CONVERT,
            opt_fn_compare: None,
            #[cfg(any(test, feature = "arbitrary"))]
            fn_arbitrary: data_type_impl::fn_f32_arb::FN_F32_ARBITRARY,
            opt_fn_param: None,
        },
        DatTypeDef {
            id: TypeFamily::F64,
            fn_base: data_type_impl::fn_f64::FN_F64_CONVERT,
            opt_fn_compare: None,
            #[cfg(any(test, feature = "arbitrary"))]
            fn_arbitrary: data_type_impl::fn_f64_arb::FN_F64_ARBITRARY,
            opt_fn_param: None,
        },
        DatTypeDef {
            id: TypeFamily::String,
            fn_base: data_type_impl::fn_string::FN_CHAR_FIXED_CONVERT,
            opt_fn_compare: Some(data_type_impl::fn_string::FN_CHAR_FIXED_COMPARE),
            #[cfg(any(test, feature = "arbitrary"))]
            fn_arbitrary: data_type_impl::fn_string_arb::FN_CHAR_FIXED_ARBITRARY,
            opt_fn_param: Some(data_type_impl::fn_string_param::FN_CHAR_FIXED_PARAM),
        },
        DatTypeDef {
            id: TypeFamily::U128,
            fn_base: data_type_impl::fn_u128::FN_OID_CONVERT,
            opt_fn_compare: Some(data_type_impl::fn_u128::FN_OID_COMPARE),
            #[cfg(any(test, feature = "arbitrary"))]
            fn_arbitrary: data_type_impl::fn_u128_arb::FN_OID_ARBITRARY,
            opt_fn_param: None,
        },
        DatTypeDef {
            id: TypeFamily::I128,
            fn_base: data_type_impl::fn_i128::FN_I128_CONVERT,
            opt_fn_compare: Some(data_type_impl::fn_i128::FN_I128_COMPARE),
            #[cfg(any(test, feature = "arbitrary"))]
            fn_arbitrary: data_type_impl::fn_i128_arb::FN_I128_ARBITRARY,
            opt_fn_param: None,
        },
        DatTypeDef {
            id: TypeFamily::Numeric,
            fn_base: data_type_impl::fn_numeric::FN_NUMERIC_CONVERT,
            opt_fn_compare: Some(data_type_impl::fn_numeric::FN_NUMERIC_COMPARE),
            #[cfg(any(test, feature = "arbitrary"))]
            fn_arbitrary: data_type_impl::fn_numeric_arb::FN_NUMERIC_ARBITRARY,
            opt_fn_param: Some(data_type_impl::fn_numeric_param::FN_NUMERIC_PARAM),
        },
        DatTypeDef {
            id: TypeFamily::Date,
            fn_base: data_type_impl::fn_date::FN_DATE_CONVERT,
            opt_fn_compare: Some(data_type_impl::fn_date::FN_DATE_COMPARE),
            #[cfg(any(test, feature = "arbitrary"))]
            fn_arbitrary: data_type_impl::fn_date_arb::FN_DATE_ARBITRARY,
            opt_fn_param: None,
        },
        DatTypeDef {
            id: TypeFamily::Time,
            fn_base: data_type_impl::fn_time::FN_TIME_CONVERT,
            opt_fn_compare: Some(data_type_impl::fn_time::FN_TIME_COMPARE),
            #[cfg(any(test, feature = "arbitrary"))]
            fn_arbitrary: data_type_impl::fn_time_arb::FN_TIME_ARBITRARY,
            opt_fn_param: Some(data_type_impl::fn_time_param::FN_TIME_PARAM),
        },
        DatTypeDef {
            id: TypeFamily::Timestamp,
            fn_base: data_type_impl::fn_timestamp::FN_TIMESTAMP_CONVERT,
            opt_fn_compare: Some(data_type_impl::fn_timestamp::FN_TIMESTAMP_COMPARE),
            #[cfg(any(test, feature = "arbitrary"))]
            fn_arbitrary: data_type_impl::fn_timestamp_arb::FN_TIMESTAMP_ARBITRARY,
            opt_fn_param: Some(data_type_impl::fn_timestamp_param::FN_TIMESTAMP_PARAM),
        },
        DatTypeDef {
            id: TypeFamily::TimestampTz,
            fn_base: data_type_impl::fn_timestamptz::FN_TIMESTAMPTZ_CONVERT,
            opt_fn_compare: Some(data_type_impl::fn_timestamptz::FN_TIMESTAMPTZ_COMPARE),
            #[cfg(any(test, feature = "arbitrary"))]
            fn_arbitrary: data_type_impl::fn_timestamptz_arb::FN_TIMESTAMPTZ_ARBITRARY,
            opt_fn_param: Some(data_type_impl::fn_timestamptz_param::FN_TIMESTAMPTZ_PARAM),
        },
        DatTypeDef {
            id: TypeFamily::Array,
            fn_base: data_type_impl::fn_array::FN_ARRAY_CONVERT,
            opt_fn_compare: None,
            #[cfg(any(test, feature = "arbitrary"))]
            fn_arbitrary: data_type_impl::fn_array_arb::FN_ARRAY_ARBITRARY,
            opt_fn_param: Some(data_type_impl::fn_array_param::FN_ARRAY_PARAM),
        },
        DatTypeDef {
            id: TypeFamily::Struct,
            fn_base: data_type_impl::fn_object::FN_OBJECT_CONVERT,
            opt_fn_compare: None,
            #[cfg(any(test, feature = "arbitrary"))]
            fn_arbitrary: data_type_impl::fn_object_arb::FN_OBJECT_ARBITRARY,
            opt_fn_param: Some(data_type_impl::fn_object_param::FN_OBJECT_PARAM),
        },
        DatTypeDef {
            id: TypeFamily::Binary,
            fn_base: data_type_impl::fn_binary::FN_BINARY_CONVERT,
            opt_fn_compare: None,
            #[cfg(any(test, feature = "arbitrary"))]
            fn_arbitrary: data_type_impl::fn_binary_arb::FN_BINARY_ARBITRARY,
            opt_fn_param: None,
        },
        // more data type definition
    ];

    static ref DAT_TYPE_DEF_REF_TABLE: Vec<Option<&'static DatTypeDef>> = {
        let len = TypeFamily::max() as usize;
        let mut vec = Vec::with_capacity(len + 1);
        vec.resize(len + 1, None);
        for _i in 0..DAT_TYPE_DEF_TABLE.len() {
            let def =  &DAT_TYPE_DEF_TABLE[_i];
            vec[def.id.to_u32() as usize] = Some(def);
        };
        vec
    };
}

pub fn get_fn_convert(id: u32) -> &'static FnBase {
    &get_dat_def(id).fn_base
}

pub fn get_opt_fn_compare(id: u32) -> &'static Option<FnCompare> {
    &get_dat_def(id).opt_fn_compare
}

#[cfg(any(test, feature = "arbitrary"))]
pub fn get_fn_arbitrary(id: u32) -> &'static FnArbitrary {
    &get_dat_def(id).fn_arbitrary
}

pub fn get_opt_fn_param(id: u32) -> &'static Option<FnParam> {
    &get_dat_def(id).opt_fn_param
}

pub fn get_fn_param(id: u32) -> Option<FnParam> {
    get_dat_def(id).opt_fn_param.clone()
}

fn get_dat_def(id: u32) -> &'static DatTypeDef {
    unsafe { DAT_TYPE_DEF_REF_TABLE[id as usize].unwrap_unchecked() }
}
