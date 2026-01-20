use crate::dat_binary::DatBinary;
use crate::dat_textual::DatTextual;
use crate::dat_type_id::DatTypeID;
use crate::datum::{Datum, DatumDyn};
use crate::dt_fn_param::DatType;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use paste::paste;
use std::fmt::Debug;
use std::hint;

/// A memory-efficient representation of data that can hold various primitive types
/// or complex types (arrays, records) in a unified enum container.
#[derive(Clone, Debug)]
pub struct DatValue {
    inner: ValueKind,
}

// Mark as thread-safe since all variants are either primitive types or boxed types
unsafe impl Send for DatValue {}
unsafe impl Sync for DatValue {}

impl AsRef<DatValue> for DatValue {
    fn as_ref(&self) -> &DatValue {
        self
    }
}

/// Internal memory representation supporting various data types
/// Uses Box for heap allocation of complex types to avoid large enum variants
enum ValueKind {
    F32(f32),
    F64(f64),
    I32(i32),
    I64(i64),
    String(String),
    Record(Vec<DatValue>),
    Array(Vec<DatValue>),
    Binary(Vec<u8>)
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
                    $(
                        ValueKind::$variant_upper(_) => {
                            DatTypeID::$variant_upper
                        }
                    )+
                }
            }
        }
        // Automatically generates debug arms for all enum variant
        impl std::fmt::Debug for ValueKind {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(
                        ValueKind::$variant_upper(value) => {
                            write!(f, "{}({:?})", stringify!($variant_upper), value)
                        }
                    )+
                }
            }
        }

        // Automatically generates clone arms for all enum variant
        impl Clone for ValueKind {
            fn clone(&self) -> Self {
                match self {
                    $(
                        ValueKind::$variant_upper(value) => {
                            Self::$variant_upper(value.clone())
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
    /// Creates a MemDatum from any type implementing Datum trait with type information
    pub fn from_datum<T: Datum>(datum: T, type_obj: &DatType) -> RS<Self> {
        Ok(Self {
            inner: ValueKind::from_datum(datum, type_obj)?,
        })
    }

    /// Conversion methods to owned values
    pub fn to_f32(&self) -> f32 {
        self.expect_f32().clone()
    }

    pub fn to_f64(&self) -> f64 {
        self.expect_f64().clone()
    }

    pub fn to_i32(&self) -> i32 {
        self.expect_i32().clone()
    }

    pub fn to_i64(&self) -> i64 {
        self.expect_i64().clone()
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
        let id = self.inner.get_dat_type_id();
        id.fn_send()(self, dat_type).map_err(|e| m_error!(EC::TypeErr, "", e))
    }

    fn to_textual(&self, dat_type: &DatType) -> RS<DatTextual> {
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
}
