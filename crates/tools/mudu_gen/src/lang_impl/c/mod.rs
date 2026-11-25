//! C language definition and Askama-based templates.
//!
//! Two back-ends share this module: the object back-end (`mgen object`,
//! freestanding guest headers over `mudu_sys.h`) and the message back-end
//! (`mgen message`, self-contained `mududb/types` headers with
//! `static inline` MessagePack codecs over the binding runtime in
//! `crates/sdk/bindings/c/mududb/codec/mpack.h`).

mod c_codec;
pub mod lang_def;
mod render_c;
mod template_enum_c;
mod template_file_c;
mod template_func_c;
mod template_object_c;
mod template_struct_c;
mod template_variant_c;
