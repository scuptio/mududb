use crate::impl_lang;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::non_primitive::NonPrimitiveType;
use mudu_binding::universal::uni_primitive::UniPrimitive;
use paste::paste;

pub mod csharp;
pub mod rust;
pub mod lang;


impl_lang! {
    (Rust, rust),
    (CSharp, csharp),
}