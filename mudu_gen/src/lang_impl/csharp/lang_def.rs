use crate::lang_impl::csharp::render_cs::create_render;
use crate::lang_impl::lang::lang_handle_tuple::lang_handle_tuple;
use crate::lang_impl::lang::non_scalar::NonScalarType;
use crate::lang_impl::lang::render::Render;
use crate::{impl_non_scalar, impl_scalar};
use mudu_binding::universal::uni_scalar::UniScalar;
use paste::paste;
use std::sync::Arc;

impl_scalar! {
    csharp,
    (Bool, "bool"),
    (U8, "byte"),
    (U16, "ushort"),
    (U32, "uint"),
    (U64, "ulong"),
    (U128, "Mudu.OID"),
    (I8, "sbyte"),
    (I16, "short"),
    (I32, "int"),
    (I64, "long"),
    (I128, "Int128"),
    (F32, "float"),
    (F64, "double"),
    (Char, "char"),
    (String, "string"),
    (Blob, "byte[]"),
    (Numeric, "string"),
    (Date, "string"),
    (Time, "string"),
    (Timestamp, "string"),
    (TimestampTz, "string"),
}

impl_non_scalar! {
    csharp,
    (Array, fn_handle_array),
    (Option, fn_handle_option),
    (Box, fn_handle_box),
    (Tuple, fn_handle_tuple),
}

fn fn_handle_array(inner: &String) -> String {
    if inner == "byte" {
        "byte[]".to_string()
    } else {
        format!("List<{}>", inner)
    }
}

fn fn_handle_option(inner: &String) -> String {
    inner.clone()
}

fn fn_handle_box(inner: &String) -> String {
    inner.to_string()
}
fn fn_handle_tuple(inner: &Vec<String>) -> String {
    lang_handle_tuple(inner)
}

pub fn create_render_cs() -> Arc<dyn Render> {
    create_render()
}
