use crate::lang_impl::assemblyscript::lang_def::create_render_as;
use crate::lang_impl::c::lang_def::create_render_c;
use crate::lang_impl::csharp::lang_def::create_render_cs;
use crate::lang_impl::go::lang_def::create_render_go;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::render::Render;
use crate::lang_impl::python::lang_def::create_render_py;
use crate::lang_impl::rust::lang_def::create_render_rs;
use std::sync::Arc;

/// Create a [`Render`] implementation for the requested language.
pub fn create_render(kind: &LangKind) -> Arc<dyn Render> {
    match kind {
        LangKind::Rust => create_render_rs(),
        LangKind::CSharp => create_render_cs(),
        LangKind::AssemblyScript => create_render_as(),
        LangKind::Python => create_render_py(),
        LangKind::C => create_render_c(),
        LangKind::Go => create_render_go(),
    }
}
