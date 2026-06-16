use crate::dat_binary::DatBinary;
use crate::dat_textual::DatTextual;
use crate::dat_type_id::DatTypeID;
use crate::datum::{Datum, DatumDyn};
use crate::dt_fn_param::DatType;
use mudu::common::result::RS;
use mudu::data_type::date::DateValue;
use mudu::data_type::numeric::Numeric;
use mudu::data_type::time::TimeValue;
use mudu::data_type::timestamp::TimestampValue;
use mudu::data_type::timestamptz::TimestampTzValue;
use mudu::error::ec::EC;
use mudu::m_error;
use paste::paste;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::hint;

/// A memory-efficient representation of data that can hold various scalar types
/// or complex types (arrays, records) in a unified enum container.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DatValue {
    inner: ValueKind,
}

// Mark as thread-safe since all variants are either scalar types or boxed types
unsafe impl Send for DatValue {}
unsafe impl Sync for DatValue {}

impl AsRef<DatValue> for DatValue {
    fn as_ref(&self) -> &DatValue {
        self
    }
}

/// Internal memory representation supporting various data types
/// Uses Box for time_series allocation of complex types to avoid large enum variants
#[derive(Clone, Debug, Serialize, Deserialize)]
enum ValueKind {
    Null,
    F32(f32),
    F64(f64),
    I32(i32),
    I64(i64),
    I128(i128),
    U128(u128),
    Numeric(Numeric),
    Date(DateValue),
    Time(TimeValue),
    Timestamp(TimestampValue),
    TimestampTz(TimestampTzValue),
    String(String),
    Record(Vec<DatValue>),
    Array(Vec<DatValue>),
    Binary(Vec<u8>),
}

macro_rules! impl_dat_value_methods {
    ($((
        $inner_type:ty,
        $variant_upper:ident,
        $variant_lower:ident
    )),+ $(,)?) => {
        $(
            impl_dat_value_methods!(
                @impl_variant
                    $inner_type,
                    $variant_upper,
                    $variant_lower
            );
        )+

        impl ValueKind {

            fn get_dat_type_id(&self) -> DatTypeID {
                match self {
                    ValueKind::Null => DatTypeID::Binary,
                    $(
                        ValueKind::$variant_upper(_) => {
                            DatTypeID::$variant_upper
                        }
                    )+
                }
            }
        }
    };

    // Handling for non-boxed types
    (@impl_variant $inner_type:ty,  $variant_upper:ident, $variant_lower:ident) => {
        paste! {
            impl DatValue {
                #[doc = "Constructor for `"]
                #[doc = stringify!($inner_type)]
                #[doc = "`"]
                pub fn [<from_ $variant_lower>](value: $inner_type) -> Self {
                    Self { inner: ValueKind::[<from_ $variant_lower>](value) }
                }

                #[doc = "Get reference to internal `"]
                #[doc = stringify!($inner_type)]
                #[doc = "` value"]
                pub fn [<as_ $variant_lower>](&self) -> Option<&$inner_type> {
                    self.inner.[<as_ $variant_lower>]()
                }

                #[doc = "Expect get reference to internal `"]
                #[doc = stringify!($inner_type)]
                #[doc = "` value"]
                pub fn [<expect_ $variant_lower>](&self) -> &$inner_type {
                    self.inner.[<expect_ $variant_lower>]()
                }

                #[doc = "Into internal `"]
                #[doc = stringify!($inner_type)]
                #[doc = "` value"]
                pub fn [<into_ $variant_lower>](self) -> $inner_type {
                    self.inner.[<into_ $variant_lower>]()
                }
            }

            impl ValueKind {
                fn [<from_ $variant_lower>](value: $inner_type) -> Self {
                    ValueKind::$variant_upper(value)
                }

                fn [<as_ $variant_lower>](&self) -> Option<&$inner_type> {
                    if let ValueKind::$variant_upper(v) = self {
                        Some(v)
                    } else {
                        None
                    }
                }

                fn [<expect_ $variant_lower>](&self) -> &$inner_type {
                    unsafe {
                        match self {
                            ValueKind::$variant_upper(value) => value,
                            _ => { hint::unreachable_unchecked() }
                        }
                    }
                }

                fn [<into_ $variant_lower>](self) -> $inner_type {
                    unsafe {
                        match self {
                            ValueKind::$variant_upper(value) => value,
                            _ => { hint::unreachable_unchecked() }
                        }
                    }
                }
            }
        }
    };
}

impl DatValue {
    pub fn null() -> Self {
        Self {
            inner: ValueKind::Null,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self.inner, ValueKind::Null)
    }

    /// Creates a MemDatum from any type implementing Datum trait with type information
    pub fn from_datum<T: Datum>(datum: T, type_obj: &DatType) -> RS<Self> {
        Ok(Self {
            inner: ValueKind::from_datum(datum, type_obj)?,
        })
    }

    /// Conversion methods to owned values
    pub fn to_f32(&self) -> f32 {
        *self.expect_f32()
    }

    pub fn to_f64(&self) -> f64 {
        *self.expect_f64()
    }

    pub fn to_i32(&self) -> i32 {
        *self.expect_i32()
    }

    pub fn to_i64(&self) -> i64 {
        *self.expect_i64()
    }

    pub fn to_i128(&self) -> i128 {
        *self.expect_i128()
    }

    pub fn to_oid(&self) -> u128 {
        *self.expect_u128()
    }
}

/// Safe wrapper for unsafe pointer casting between types
/// Assumes the caller guarantees type compatibility
#[inline]
#[allow(unused)]
fn unsafe_cast<S, D>(src: &S) -> &D {
    unsafe { &*(src as *const S as *const D) }
}

impl ValueKind {
    /// Internal method to create ValueKind from Datum with type information
    fn from_datum<T: Datum>(datum: T, type_obj: &DatType) -> RS<Self> {
        Ok(datum.to_value(type_obj)?.inner)
    }
}

// Mark internal enum as thread-safe since all variants are either primitive or boxed
unsafe impl Send for ValueKind {}
unsafe impl Sync for ValueKind {}

impl_dat_value_methods! {
    (i32, I32, i32),
    (i64, I64, i64),
    (i128, I128, i128),
    (u128, U128, u128),
    (Numeric, Numeric, numeric),
    (DateValue, Date, date),
    (TimeValue, Time, time),
    (TimestampValue, Timestamp, timestamp),
    (TimestampTzValue, TimestampTz, timestamptz),
    (f32, F32, f32),
    (f64, F64, f64),
    (String, String, string),
    (Vec<DatValue>, Array, array),
    (Vec<DatValue>, Record, record),
    (Vec<u8>, Binary, binary),
}

impl DatumDyn for DatValue {
    fn dat_type_id(&self) -> RS<DatTypeID> {
        Ok(self.inner.get_dat_type_id())
    }

    fn to_binary(&self, dat_type: &DatType) -> RS<DatBinary> {
        if self.is_null() {
            return Err(m_error!(EC::TypeErr, "NULL has no binary payload"));
        }
        let id = self.inner.get_dat_type_id();
        id.fn_send()(self, dat_type).map_err(|e| m_error!(EC::TypeErr, "", e))
    }

    fn to_textual(&self, dat_type: &DatType) -> RS<DatTextual> {
        if self.is_null() {
            return Ok(DatTextual::from("NULL".to_string()));
        }
        let id = self.inner.get_dat_type_id();
        id.fn_output()(self, dat_type).map_err(|e| m_error!(EC::TypeErr, "", e))
    }

    fn to_value(&self, _: &DatType) -> RS<DatValue> {
        Ok(self.clone())
    }

    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use crate::dat_value::DatValue;
    use serde_json::json;

    #[test]
    fn test() {
        let s = "string";
        let mem = DatValue::from_string(s.to_string());
        assert_eq!(mem.as_string(), Some(&s.to_string()));
        assert_eq!(mem.expect_string(), &s.to_string());
        assert!(mem.as_i32().is_none());

        let i = 10;
        let mem = DatValue::from_i32(i);
        assert_eq!(mem.as_i32(), Some(&i));
        assert_eq!(mem.expect_i32(), &i);
        assert!(mem.as_string().is_none());
    }

    #[test]
    fn serde_roundtrip_json() {
        let value = DatValue::from_record(vec![
            DatValue::from_i32(7),
            DatValue::from_string("hello".to_string()),
            DatValue::from_array(vec![
                DatValue::from_i64(9),
                DatValue::from_binary(vec![1, 2, 3]),
            ]),
        ]);

        let json_value = serde_json::to_value(&value).unwrap();
        assert_eq!(
            json_value,
            json!({
                "inner": {
                    "Record": [
                        {"inner": {"I32": 7}},
                        {"inner": {"String": "hello"}},
                        {"inner": {"Array": [
                            {"inner": {"I64": 9}},
                            {"inner": {"Binary": [1, 2, 3]}}
                        ]}}
                    ]
                }
            })
        );

        let from_json: DatValue = serde_json::from_value(json_value).unwrap();
        assert_eq!(from_json.expect_record().len(), 3);
    }
}
