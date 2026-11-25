use crate::data_type_param::DataTypeParamDyn;
use crate::data_type_param_array::DataTypeParamArray;
use crate::data_type_param_numeric::DataTypeParamNumeric;
use crate::data_type_param_string::DataTypeParamString;
use crate::data_type_param_struct::DataTypeParamStruct;
use crate::data_type_param_time::DataTypeParamTime;
use crate::data_type_param_timestamp::DataTypeParamTimestamp;
use crate::data_type_param_timestamptz::DataTypeParamTimestampTz;
use crate::type_family::{TypeFamily, TypeName};
use mudu::common::cmp_order::Order;
use mudu::common::result::RS;
use paste::paste;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt::Debug;

/// The parameter object of a parameterized data type, one variant per
/// parameterized type family.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DataTypeParamKind<T> {
    /// Parameter of the `String` (`varchar`) family.
    String(Box<DataTypeParamString>),
    /// Parameter of the `Numeric` family.
    Numeric(Box<DataTypeParamNumeric>),
    /// Parameter of the `Time` family.
    Time(Box<DataTypeParamTime>),
    /// Parameter of the `Timestamp` family.
    Timestamp(Box<DataTypeParamTimestamp>),
    /// Parameter of the `TimestampTz` family.
    TimestampTz(Box<DataTypeParamTimestampTz>),
    /// Parameter of the `Struct` family.
    Struct(Box<DataTypeParamStruct<T>>),
    /// Parameter of the `Array` family.
    Array(Box<DataTypeParamArray<T>>),
}

macro_rules! impl_dtp_kind_methods {
    ($((
        $inner_type:ty,
        $variant_upper:ident,
        $variant_lower:ident
    )),+ $(,)?) => {
        impl<T> DataTypeParamKind<T> {
            /// Returns the type family this parameter belongs to.
            pub fn type_family(&self) -> TypeFamily {
                match self {
                    $(DataTypeParamKind::$variant_upper(_) => { TypeFamily::$variant_upper })*
                }
            }
        }

        impl<T: Clone + Serialize + DeserializeOwned + TypeName + Debug + Send + Sync + 'static> DataTypeParamKind<T> {
            paste! {
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
            }

            /// Returns the parameter object as a `dyn DataTypeParamDyn`.
            pub fn as_dtp_dyn(&self) -> & dyn DataTypeParamDyn {
                match self {
                    $(DataTypeParamKind::$variant_upper(p) => { p.as_ref() })*
                }
            }

            /// Returns the SQL display name of the parameterized type.
            pub fn name(&self) -> String {
                match self {
                    $(DataTypeParamKind::$variant_upper(inner) => { inner.name() })*
                }
            }
        }

        impl<T: Order> DataTypeParamKind<T> {
            /// Compares two parameters; parameters of different families
            /// order by their type family.
            pub fn compare(&self, other: &Self) -> RS<Ordering> {
                let ord = match (self, other) {
                    $((DataTypeParamKind::$variant_upper(l), DataTypeParamKind::$variant_upper(r)) => { l.cmp_ord(r)? })*
                    _ => { self.type_family().cmp(&other.type_family()) }
                };
                Ok(ord)
            }
        }

        $(
            paste! {
                impl<T> DataTypeParamKind<T> {
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

impl<T: Order> Order for DataTypeParamKind<T> {
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
    (DataTypeParamStruct<T>, Struct, object),
    (DataTypeParamArray<T>, Array, array),
}
