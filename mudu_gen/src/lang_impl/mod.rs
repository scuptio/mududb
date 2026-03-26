use crate::impl_lang;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::non_primitive::NonPrimitiveType;
use mudu_binding::universal::uni_primitive::UniPrimitive;
use paste::paste;

pub mod csharp;
pub mod lang;
pub mod rust;

impl_lang! {
    (Rust, rust),
    (CSharp, csharp),
}
