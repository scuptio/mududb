use std::sync::Arc;
use crate::lang_impl::lang::lang_handle_tuple::lang_handle_tuple;
use crate::lang_impl::lang::non_primitive::NonPrimitiveType;
use crate::{impl_non_primitive, impl_primitive};
use mudu_binding::universal::uni_primitive::UniPrimitive;
use paste::paste;
use crate::lang_impl::lang::render::Render;
use crate::lang_impl::rust::render_rs::create_render;

impl_primitive! {
    rust,
    (Bool, "bool"),
    (U8, "u8"),
    (U16, "u16"),
    (U32, "u32"),
    (U64, "u64"),
    (I8, "i8"),
    (I16, "i16"),
    (I32, "i32"),
    (I64, "i64"),
    (F32, "f32"),
    (F64, "f64"),
    (Char, "char"),
    (String, "String"),
}


impl_non_primitive! {
    rust,
    (Array, fn_handle_array),
    (Option, fn_handle_option),
    (Box, fn_handle_box),
    (Tuple, fn_handle_tuple),
}

fn fn_handle_array(inner:&String) -> String {
    format!("Vec<{}>", inner)
}

fn fn_handle_option(inner:&String) -> String {
    format!("Option<{}>", inner)
}

fn fn_handle_tuple(inner:&Vec<String>) -> String {
    lang_handle_tuple(inner)
}


fn fn_handle_box(inner:&String) -> String {
    format!("Box<{}>", inner)
}

pub fn create_render_rs() -> Arc<dyn Render> {
    create_render()
}