use crate::dat_type_id::DatTypeID;
use crate::dat_value::DatValue;
use paste::paste;

pub enum DatValArray {
    I32(Vec<i32>),
    I64(Vec<i64>),
    F32(Vec<f32>),
    F64(Vec<f64>),
    String(Vec<String>),
    Record(Vec<Vec<DatValue>>),
    Array(Vec<Vec<DatValArray>>),
}

macro_rules! impl_dat_val_array_methods {
    ($((
        $inner_type:ty,
        $variant_upper:ident,
        $variant_lower:ident
     )),+
    $(,)?) => {
        impl DatValArray {
            pub fn get_dat_type_id(&self) -> DatTypeID {
                match self {
                    $(
                        Self::$variant_upper(_) => {
                            DatTypeID::$variant_upper
                        }
                    )+
                }
            }
        }
        // Automatically generates debug arms for all enum variant
        impl std::fmt::Debug for DatValArray {
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
        impl Clone for DatValArray {
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
            impl_dat_val_array_methods!(
                @impl_variant
                    $inner_type,
                    $variant_upper,
                    $variant_lower
            );
        )+

    };

    (@impl_variant $inner_type:ty,  $variant_upper:ident, $variant_lower:ident) => {
        paste! {
            impl DatValArray {
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

impl_dat_val_array_methods! {
    (Vec<i32>, I32, i32),
    (Vec<i64>, I64, i64),
    (Vec<f32>, F32, f32),
    (Vec<f64>, F64, f64),
    (Vec<String>, String, string),
    (Vec<Vec<DatValArray>>, Array, array),
    (Vec<Vec<DatValue>>, Record, object)
}
