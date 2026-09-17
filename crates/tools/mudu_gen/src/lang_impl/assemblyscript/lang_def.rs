//! AssemblyScript scalar/non-scalar type mappings.

use crate::lang_impl::assemblyscript::render_as::create_render;
use crate::lang_impl::lang::non_scalar::NonScalarType;
use crate::lang_impl::lang::render::Render;
use crate::{impl_non_scalar, impl_scalar};
use mudu_binding::universal::uni_scalar::UniScalar;
use paste::paste;
use std::sync::Arc;

impl_scalar! {
    assemblyscript,
    (Bool, "bool"),
    (U8, "u8"),
    (U16, "u16"),
    (U32, "u32"),
    (U64, "u64"),
    (U128, "u128"),
    (I8, "i8"),
    (I16, "i16"),
    (I32, "i32"),
    (I64, "i64"),
    (I128, "i128"),
    (F32, "f32"),
    (F64, "f64"),
    (Char, "string"),
    (String, "string"),
    (Blob, "Uint8Array"),
    (Numeric, "string"),
    (Date, "string"),
    (Time, "string"),
    (Timestamp, "string"),
    (TimestampTz, "string"),
}

impl_non_scalar! {
    assemblyscript,
    (Array, fn_handle_array),
    (Option, fn_handle_option),
    (Box, fn_handle_box),
    (Tuple, fn_handle_tuple),
}

fn fn_handle_array(inner: &String) -> String {
    if inner == "u8" {
        "Uint8Array".to_string()
    } else {
        format!("Array<{}>", inner)
    }
}

fn fn_handle_option(inner: &str) -> String {
    format!("{} | null", inner)
}

fn fn_handle_box(inner: &String) -> String {
    inner.to_string()
}

fn fn_handle_tuple(inner: &[String]) -> String {
    // AssemblyScript does not have heterogeneous tuples; represent as an untyped
    // array. Project-controlled codecs for tuple fields are generated element-wise.
    let _ = inner;
    "Array<any>".to_string()
}

/// Create the AssemblyScript rendering back-end.
pub fn create_render_as() -> Arc<dyn Render> {
    create_render()
}
