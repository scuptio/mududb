use crate::data_binary::DataBinary;
use crate::data_textual::DataTextual;
use crate::data_type_fn_param::DataType;
use crate::datum::{Datum, DatumDyn};
use crate::type_family::TypeFamily;
use mudu::common::result::RS;
use mudu::data_type::date::DateValue;
use mudu::data_type::numeric::Numeric;
use mudu::data_type::time::TimeValue;
use mudu::data_type::timestamp::TimestampValue;
use mudu::data_type::timestamptz::TimestampTzValue;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use paste::paste;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::hint;

/// A memory-efficient representation of data that can hold various scalar types
/// or complex types (arrays, records) in a unified enum container.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataValue {
    inner: ValueKind,
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
    Record(Vec<DataValue>),
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

            fn get_type_family(&self) -> TypeFamily {
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

impl DataValue {
    pub fn null() -> Self {
        Self {
            inner: ValueKind::Null,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self.inner, ValueKind::Null)
    }

    /// Creates a MemDatum from any type implementing Datum trait with type information
    pub fn from_datum<T: Datum>(datum: T, type_obj: &DataType) -> RS<Self> {
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

impl ValueKind {
    /// Internal method to create ValueKind from Datum with type information
    fn from_datum<T: Datum>(datum: T, type_obj: &DataType) -> RS<Self> {
        Ok(datum.to_value(type_obj)?.inner)
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
    (Vec<DataValue>, Record, record),
    (Vec<u8>, Binary, binary),
}

impl DatumDyn for DataValue {
    fn type_family(&self) -> RS<TypeFamily> {
        Ok(self.inner.get_type_family())
    }

    fn to_binary(&self, data_type: &DataType) -> RS<DataBinary> {
        if self.is_null() {
            return Err(mudu_error!(
                ErrorCode::InvalidType,
                "NULL has no binary payload"
            ));
        }
        let id = self.inner.get_type_family();
        id.fn_send()(self, data_type)
            .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, "", e))
    }

    fn to_textual(&self, data_type: &DataType) -> RS<DataTextual> {
        if self.is_null() {
            return Ok(DataTextual::from("NULL".to_string()));
        }
        let id = self.inner.get_type_family();
        id.fn_output()(self, data_type)
            .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, "", e))
    }

    fn to_value(&self, _: &DataType) -> RS<DataValue> {
        Ok(self.clone())
    }

    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
        Box::new(self.clone())
    }
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
        let value = DataValue::from_record(vec![
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

        let from_json: DataValue = serde_json::from_value(json_value).unwrap();
        assert_eq!(from_json.expect_record().len(), 3);
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
        assert!(array_value.as_record().is_none());
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

        let record = DataValue::from_record(vec![
            DataValue::from_string("x".to_string()),
            DataValue::from_i64(9),
        ]);
        assert_eq!(record.as_record().unwrap().len(), 2);
        assert_eq!(record.expect_record()[1].to_i64(), 9);
    }
}
