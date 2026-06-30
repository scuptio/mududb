//! Parsers and generators that turn WIT files and DDL SQL into source code.

pub mod code_gen;
pub mod codegen_cfg;
mod test_mudul_parse;
pub mod wit_parser;
#[cfg(all(test, not(miri)))]
mod wit_parser_test;

mod create_render;
pub mod gen_entity;
#[cfg(test)]
mod gen_entity_test;
pub mod gen_message;
#[cfg(test)]
mod gen_message_test;
mod wit_def;
