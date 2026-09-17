//! C language definition and Askama-based templates.
//!
//! Two back-ends share this module: the entity back-end (`mgen entity`,
//! freestanding guest headers over `mudu_sys.h`) and the message back-end
//! (`mgen message`, self-contained `mududb/types` headers with
//! `static inline` MessagePack codecs over the binding runtime in
//! `crates/sdk/bindings/c/mududb/codec/mpack.h`).

mod c_codec;
pub mod lang_def;
mod render_c;
mod template_entity_c;
mod template_enum_c;
mod template_file_c;
mod template_func_c;
mod template_record_c;
mod template_variant_c;
