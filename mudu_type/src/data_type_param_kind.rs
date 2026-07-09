use crate::data_type_param::DataTypeParamDyn;
use crate::data_type_param_array::DataTypeParamArray;
use crate::data_type_param_numeric::DataTypeParamNumeric;
use crate::data_type_param_record::DataTypeParamRecord;
use crate::data_type_param_string::DataTypeParamString;
use crate::data_type_param_time::DataTypeParamTime;
use crate::data_type_param_timestamp::DataTypeParamTimestamp;
use crate::data_type_param_timestamptz::DataTypeParamTimestampTz;
use crate::type_family::TypeFamily;
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use paste::paste;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DataTypeParamKind {
    String(Box<DataTypeParamString>),
    Numeric(Box<DataTypeParamNumeric>),
    Time(Box<DataTypeParamTime>),
    Timestamp(Box<DataTypeParamTimestamp>),
    TimestampTz(Box<DataTypeParamTimestampTz>),
    Record(Box<DataTypeParamRecord>),
    Array(Box<DataTypeParamArray>),
}

impl DataTypeParamKind {}

macro_rules! impl_dtp_kind_methods {
    ($((
        $inner_type:ty,
        $variant_upper:ident,
        $variant_lower:ident
    )),+ $(,)?) => {
        paste! {
            impl DataTypeParamKind {
                #[doc = "map inner `"]
                #[doc = "`"]
                pub fn map<U, F>(&self, f: F) -> U
                where
                    F: FnOnce(&dyn DataTypeParamDyn) -> U,
                {
                    match self {
                        $(DataTypeParamKind::$variant_upper(p) => { f(p.as_ref()) })*
                    }
                }

                pub fn type_family(&self) -> TypeFamily {
                    match self {
                        $(DataTypeParamKind::$variant_upper(_) => { TypeFamily::$variant_upper })*
                    }
                }

                pub fn as_dtp_dyn(&self) -> & dyn DataTypeParamDyn {
                    match self {
                        $(DataTypeParamKind::$variant_upper(p) => { p.as_ref() })*
                    }
                }

                pub fn compare(&self, other: &Self) -> RS<Ordering> {
                    let ord = match (self, other) {
                        $((DataTypeParamKind::$variant_upper(l), DataTypeParamKind::$variant_upper(r)) => { l.cmp_ord(r)? })*
                        _ => { self.type_family().cmp(&other.type_family()) }
                    };
                    Ok(ord)
                }

                pub fn name(&self) -> String {
                    let name = match self {
                        $(
                            DataTypeParamKind::$variant_upper(inner) => { inner.name() }
                        )*
                    };
                    name
                }
            }
        }
        $(
            paste! {
                impl DataTypeParamKind {
                    #[doc = "Get reference to internal type`"]
                    #[doc = stringify!($inner_type)]
                    #[doc = "` value"]
                    #[inline]
                    pub fn [<as_ $variant_lower _param>](&self) -> Option<&$inner_type> {
                        match self {
                            DataTypeParamKind::$variant_upper(v) => { Some(v.as_ref()) },
                            _ => { None }
                        }
                    }

                    #[doc = "Expect get reference to internal `"]
                    #[doc = stringify!($inner_type)]
                    #[doc = "` value"]
                    #[inline]
                    pub fn [<expect_ $variant_lower _param>](&self) -> &$inner_type {
                        self.[<as_ $variant_lower _param>]().unwrap()
                    }
                }
            }
        )+
    };
}

impl Order for DataTypeParamKind {
    fn cmp_ord(&self, other: &Self) -> RS<Ordering> {
        self.compare(other)
    }
}

impl_dtp_kind_methods! {
    (DataTypeParamString, String, string),
    (DataTypeParamNumeric, Numeric, numeric),
    (DataTypeParamTime, Time, time),
    (DataTypeParamTimestamp, Timestamp, timestamp),
    (DataTypeParamTimestampTz, TimestampTz, timestamptz),
    (DataTypeParamRecord, Record, object),
    (DataTypeParamArray, Array, array),
}
