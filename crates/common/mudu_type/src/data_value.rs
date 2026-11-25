use crate::type_family::TypeFamily;
use mudu::data_type::date::DateValue;
use mudu::data_type::numeric::Numeric;
use mudu::data_type::time::TimeValue;
use mudu::data_type::timestamp::TimestampValue;
use mudu::data_type::timestamptz::TimestampTzValue;
use paste::paste;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// A memory-efficient representation of data that can hold various scalar types
/// or complex types (arrays, records) in a unified enum container.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataValue {
    pub(crate) inner: ValueKind,
}

// Mark as thread-safe since all variants are either scalar types or boxed types
unsafe impl Send for DataValue {}
unsafe impl Sync for DataValue {}

impl AsRef<DataValue> for DataValue {
    fn as_ref(&self) -> &DataValue {
        self
    }
}

/// Internal memory representation supporting various data types
/// Uses Box for time_series allocation of complex types to avoid large enum variants
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) enum ValueKind {
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
    Struct(Vec<DataValue>),
    Array(Vec<DataValue>),
    Binary(Vec<u8>),
}

macro_rules! impl_data_value_methods {
    ($((
        $inner_type:ty,
        $variant_upper:ident,
        $variant_lower:ident
    )),+ $(,)?) => {
        $(
            impl_data_value_methods!(
                @impl_variant
                    $inner_type,
                    $variant_upper,
                    $variant_lower
            );
        )+

        impl ValueKind {

            pub(crate) fn get_type_family(&self) -> TypeFamily {
                match self {
                    ValueKind::Null => TypeFamily::Binary,
                    $(
                        ValueKind::$variant_upper(_) => {
                            TypeFamily::$variant_upper
                        }
                    )+
                }
            }
        }
    };

    // Handling for non-boxed types
    (@impl_variant $inner_type:ty,  $variant_upper:ident, $variant_lower:ident) => {
        paste! {
            impl DataValue {
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
                    match self {
                        ValueKind::$variant_upper(value) => value,
                        other => panic!(
                            "expected {}, got {:?}",
                            stringify!($variant_upper),
                            other
                        ),
                    }
                }

                fn [<into_ $variant_lower>](self) -> $inner_type {
                    match self {
                        ValueKind::$variant_upper(value) => value,
                        other => panic!(
                            "expected {}, got {:?}",
                            stringify!($variant_upper),
                            other
                        ),
                    }
                }
            }
        }
    };
}

impl DataValue {
    /// Creates the NULL value.
    pub fn null() -> Self {
        Self {
            inner: ValueKind::Null,
        }
    }

    /// Returns true if this value is NULL.
    pub fn is_null(&self) -> bool {
        matches!(self.inner, ValueKind::Null)
    }

    /// Conversion methods to owned values
    pub fn to_f32(&self) -> f32 {
        *self.expect_f32()
    }

    /// Returns the contained `f64`; panics if the value is not an `F64`.
    pub fn to_f64(&self) -> f64 {
        *self.expect_f64()
    }

    /// Returns the contained `i32`; panics if the value is not an `I32`.
    pub fn to_i32(&self) -> i32 {
        *self.expect_i32()
    }

    /// Returns the contained `i64`; panics if the value is not an `I64`.
    pub fn to_i64(&self) -> i64 {
        *self.expect_i64()
    }

    /// Returns the contained `i128`; panics if the value is not an `I128`.
    pub fn to_i128(&self) -> i128 {
        *self.expect_i128()
    }

    /// Returns the contained OID (`u128`); panics if the value is not a `U128`.
    pub fn to_oid(&self) -> u128 {
        *self.expect_u128()
    }
}

// Mark internal enum as thread-safe since all variants are either primitive or boxed
unsafe impl Send for ValueKind {}
unsafe impl Sync for ValueKind {}

impl_data_value_methods! {
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
    (Vec<DataValue>, Array, array),
    (Vec<DataValue>, Struct, struct),
    (Vec<u8>, Binary, binary),
}

#[cfg(test)]
mod tests {
    use crate::data_value::DataValue;
    use crate::datum::DatumDyn;
    use crate::type_family::TypeFamily;
    use serde_json::json;

    #[test]
    fn test() {
        let s = "string";
        let mem = DataValue::from_string(s.to_string());
        assert_eq!(mem.as_string(), Some(&s.to_string()));
        assert_eq!(mem.expect_string(), &s.to_string());
        assert!(mem.as_i32().is_none());

        let i = 10;
        let mem = DataValue::from_i32(i);
        assert_eq!(mem.as_i32(), Some(&i));
        assert_eq!(mem.expect_i32(), &i);
        assert!(mem.as_string().is_none());
    }

    #[test]
    fn serde_roundtrip_json() {
        let value = DataValue::from_struct(vec![
            DataValue::from_i32(7),
            DataValue::from_string("hello".to_string()),
            DataValue::from_array(vec![
                DataValue::from_i64(9),
                DataValue::from_binary(vec![1, 2, 3]),
            ]),
        ]);

        let json_value = serde_json::to_value(&value).unwrap();
        assert_eq!(
            json_value,
            json!({
                "inner": {
                    "Struct": [
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

        let from_json: DataValue = serde_json::from_value(json_value).unwrap();
        assert_eq!(from_json.expect_struct().len(), 3);
    }

    #[test]
    fn wrong_accessor_returns_none() {
        let i32_value = DataValue::from_i32(42);
        assert!(i32_value.as_string().is_none());
        assert!(i32_value.as_i64().is_none());
        assert!(i32_value.as_f64().is_none());
        assert!(i32_value.as_array().is_none());

        let string_value = DataValue::from_string("hello".to_string());
        assert!(string_value.as_i32().is_none());
        assert!(string_value.as_binary().is_none());

        let array_value = DataValue::from_array(vec![DataValue::from_i32(1)]);
        assert!(array_value.as_struct().is_none());
        assert!(array_value.as_i32().is_none());
    }

    #[test]
    fn null_value_is_null() {
        let null = DataValue::null();
        assert!(null.is_null());
    }

    #[test]
    fn null_has_no_type_family_matches_binary() {
        let null = DataValue::null();
        assert_eq!(null.type_family().unwrap(), TypeFamily::Binary);
    }

    #[test]
    fn array_and_record_constructors_round_trip() {
        let array = DataValue::from_array(vec![DataValue::from_i32(1), DataValue::from_i32(2)]);
        assert_eq!(array.as_array().unwrap().len(), 2);
        assert_eq!(array.expect_array()[0].to_i32(), 1);

        let record = DataValue::from_struct(vec![
            DataValue::from_string("x".to_string()),
            DataValue::from_i64(9),
        ]);
        assert_eq!(record.as_struct().unwrap().len(), 2);
        assert_eq!(record.expect_struct()[1].to_i64(), 9);
    }

    #[test]
    fn owned_scalar_accessors_roundtrip() {
        assert_eq!(DataValue::from_f32(1.5).to_f32(), 1.5);
        assert_eq!(DataValue::from_f64(2.5).to_f64(), 2.5);
        assert_eq!(DataValue::from_i32(3).to_i32(), 3);
        assert_eq!(DataValue::from_i64(4).to_i64(), 4);
        assert_eq!(DataValue::from_i128(5).to_i128(), 5);
        assert_eq!(DataValue::from_u128(6).to_oid(), 6);
    }

    #[test]
    fn as_ref_returns_self() {
        let value = DataValue::from_i32(7);
        let borrowed: &DataValue = value.as_ref();
        assert_eq!(borrowed.to_i32(), 7);
    }

    #[test]
    fn datum_dyn_binary_and_textual_roundtrip() {
        let data_type = crate::data_type::DataType::default_for(TypeFamily::I32);
        let value = DataValue::from_i32(42);

        assert!(value.to_binary(&data_type).is_ok());
        assert!(value.to_textual(&data_type).is_ok());
        assert_eq!(value.to_value(&data_type).unwrap().to_i32(), 42);
        assert_eq!(value.clone_boxed().type_family().unwrap(), TypeFamily::I32);
    }

    #[test]
    fn null_binary_is_rejected_and_textual_is_null() {
        let data_type = crate::data_type::DataType::default_for(TypeFamily::I32);
        let null = DataValue::null();
        assert!(null.to_binary(&data_type).is_err());
        assert!(null.to_textual(&data_type).is_ok());
        assert!(null.to_value(&data_type).unwrap().is_null());
    }
}
