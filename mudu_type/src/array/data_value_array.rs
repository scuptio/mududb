use crate::data_value::DataValue;
use crate::type_family::TypeFamily;
use paste::paste;

pub enum DataValueArray {
    I32(Vec<i32>),
    I64(Vec<i64>),
    F32(Vec<f32>),
    F64(Vec<f64>),
    String(Vec<String>),
    Record(Vec<Vec<DataValue>>),
    Array(Vec<Vec<DataValueArray>>),
}

macro_rules! impl_data_value_array_methods {
    ($((
        $inner_type:ty,
        $variant_upper:ident,
        $variant_lower:ident
     )),+
    $(,)?) => {
        impl DataValueArray {
            pub fn get_type_family(&self) -> TypeFamily {
                match self {
                    $(
                        Self::$variant_upper(_) => {
                            TypeFamily::$variant_upper
                        }
                    )+
                }
            }
        }
        // Automatically generates debug arms for all enum variant
        impl std::fmt::Debug for DataValueArray {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(
                        Self::$variant_upper(value) => {
                            write!(f, "{}({:?})", stringify!($variant_upper), value)
                        }
                    )+
                }
            }
        }

        // Automatically generates clone arms for all enum variant
        impl Clone for DataValueArray {
            fn clone(&self) -> Self {
                match self {
                    $(
                        Self::$variant_upper(value) => {
                            Self::$variant_upper(value.clone())
                        }
                    )+
                }
            }
        }

        $(
            impl_data_value_array_methods!(
                @impl_variant
                    $inner_type,
                    $variant_upper,
                    $variant_lower
            );
        )+

    };

    (@impl_variant $inner_type:ty,  $variant_upper:ident, $variant_lower:ident) => {
        paste! {
            impl DataValueArray {
                #[doc = "Constructor for `"]
                #[doc = stringify!($variant_lower)]
                #[doc = "` array"]
                pub fn [<from_ $variant_lower>](value: $inner_type) -> Self {
                    Self:: $variant_upper(value)
                }

                #[doc = "Get reference to internal `"]
                #[doc = stringify!($variant_lower)]
                #[doc = "` array"]
                pub fn [<as_ $variant_lower>](&self) -> Option<&$inner_type> {
                    match self {
                        Self::$variant_upper(value) => Some(value),
                        _ => { None }
                    }
                }

                #[doc = "Expect get reference to internal `"]
                #[doc = stringify!($variant_lower)]
                #[doc = "` array"]
                pub fn [<expect_ $variant_lower>](&self) -> &$inner_type {
                    unsafe {
                        match self {
                            Self::$variant_upper(value) => value,
                            _ => { std::hint::unreachable_unchecked() }
                        }
                    }
                }
            }
        }
    };
}

impl_data_value_array_methods! {
    (Vec<i32>, I32, i32),
    (Vec<i64>, I64, i64),
    (Vec<f32>, F32, f32),
    (Vec<f64>, F64, f64),
    (Vec<String>, String, string),
    (Vec<Vec<DataValueArray>>, Array, array),
    (Vec<Vec<DataValue>>, Record, object)
}
