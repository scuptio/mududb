//! Go (TinyGo guest) language definition and Askama-based templates.

pub mod go_codec;
pub mod lang_def;
mod render_go;
mod template_enum_go;
mod template_file_go;
mod template_func_go;
mod template_object_go;
mod template_struct_go;
mod template_variant_go;

#[cfg(test)]
mod go_codec_test;
#[cfg(test)]
mod template_message_go_test;
