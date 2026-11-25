//! Parsers and generators that turn WIT files and DDL SQL into source code.

pub mod canonical;
pub mod code_gen;
pub mod codegen_cfg;
mod test_mudul_parse;
pub mod wit_emit;
pub mod wit_parser;
#[cfg(all(test, not(miri)))]
mod wit_parser_test;

mod create_render;
pub mod gen_message;
#[cfg(test)]
mod gen_message_test;
pub mod gen_object;
#[cfg(test)]
mod gen_object_test;
#[cfg(all(test, not(miri)))]
mod gen_syscall_roundtrip_test;
#[cfg(test)]
mod render_kinds_test;
pub mod schema_desc;
/// Parsed WIT definitions shared by the parsers and code generators.
pub mod wit_def;
