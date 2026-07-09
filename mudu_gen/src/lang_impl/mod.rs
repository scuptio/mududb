//! Concrete language implementations and the shared language abstraction.

use crate::impl_lang;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::non_scalar::NonScalarType;
use mudu_binding::universal::uni_scalar::UniScalar;
use paste::paste;

pub mod assemblyscript;
pub mod csharp;
pub mod lang;
pub mod rust;

impl_lang! {
    (Rust, rust),
    (CSharp, csharp),
    (AssemblyScript, assemblyscript),
}
