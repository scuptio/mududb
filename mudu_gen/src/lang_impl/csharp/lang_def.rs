use std::sync::Arc;
use crate::lang_impl::lang::lang_handle_tuple::lang_handle_tuple;
use crate::lang_impl::lang::non_primitive::NonPrimitiveType;
use crate::{impl_non_primitive, impl_primitive};
use mudu_binding::universal::uni_primitive::UniPrimitive;
use paste::paste;
use crate::lang_impl::csharp::render_cs::create_render;
use crate::lang_impl::lang::render::Render;

impl_primitive! {
    csharp,
    (Bool, "bool"),
    (U8, "byte"),
    (U16, "ushort"),
    (U32, "uint"),
    (U64, "ulong"),
    (I8, "sbyte"),
    (I16, "short"),
    (I32, "int"),
    (I64, "long"),
    (F32, "float"),
    (F64, "double"),
    (Char, "char"),
    (String, "string"),
}


impl_non_primitive! {
    csharp,
    (Array, fn_handle_array),
    (Option, fn_handle_option),
    (Box, fn_handle_box),
    (Tuple, fn_handle_tuple),
}



fn fn_handle_array(inner:&String) -> String {
    if inner == "byte" {
        "byte[]".to_string()
    } else {
        format!("List<{}>", inner)
    }
}

fn fn_handle_option(inner:&String) -> String {
    inner.clone()
}

fn fn_handle_box(inner:&String) -> String {
    inner.to_string()
}
fn fn_handle_tuple(inner:&Vec<String>) -> String {
    lang_handle_tuple(inner)
}


pub fn create_render_cs() -> Arc<dyn Render> {
    create_render()
}