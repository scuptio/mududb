//! Python scalar/non-scalar type mappings.

use crate::lang_impl::lang::non_scalar::NonScalarType;
use crate::lang_impl::lang::render::Render;
use crate::lang_impl::python::render_py::create_render;
use crate::{impl_non_scalar, impl_scalar};
use mudu_binding::universal::uni_scalar::UniScalar;
use paste::paste;
use std::sync::Arc;

impl_scalar! {
    python,
    (Bool, "bool"),
    (U8, "int"),
    (U16, "int"),
    (U32, "int"),
    (U64, "int"),
    (U128, "int"),
    (I8, "int"),
    (I16, "int"),
    (I32, "int"),
    (I64, "int"),
    (I128, "int"),
    (F32, "float"),
    (F64, "float"),
    (Char, "str"),
    (String, "str"),
    (Blob, "bytes"),
    (Numeric, "str"),
    (Date, "str"),
    (Time, "str"),
    (Timestamp, "str"),
    (TimestampTz, "str"),
}

impl_non_scalar! {
    python,
    (Array, fn_handle_array),
    (Option, fn_handle_option),
    (Box, fn_handle_box),
    (Tuple, fn_handle_tuple),
}

fn fn_handle_array(inner: &String) -> String {
    format!("list[{}]", inner)
}

fn fn_handle_option(inner: &str) -> String {
    // PEP 604 `X | None` needs Python 3.10; `Optional[X]` keeps the generated
    // code importable on 3.9.
    format!("Optional[{}]", inner)
}

fn fn_handle_box(inner: &String) -> String {
    inner.to_string()
}

fn fn_handle_tuple(inner: &[String]) -> String {
    format!("tuple[{}]", inner.join(", "))
}

/// Create the Python rendering back-end.
pub fn create_render_py() -> Arc<dyn Render> {
    create_render()
}
