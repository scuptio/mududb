use std::sync::Arc;
use crate::lang_impl::csharp::lang_def::create_render_cs;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::render::Render;
use crate::lang_impl::rust::lang_def::create_render_rs;

pub fn create_render(kind:&LangKind) -> Arc<dyn Render> {
    match kind {
        LangKind::Rust => { create_render_rs() }
        LangKind::CSharp => { create_render_cs() }
    }
}