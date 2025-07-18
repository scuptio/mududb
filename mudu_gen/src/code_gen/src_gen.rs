use crate::code_gen::rust_builder::RustBuilder;
use crate::code_gen::src_builder::SrcBuilder;
use crate::code_gen::table_def::TableDef;
use clap::ValueEnum;
use mudu::common::result::RS;
use std::sync::Arc;

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum Language {
    Rust,
}

pub struct SrcGen {}

impl SrcGen {
    pub fn new() -> Self {
        Self {}
    }

    pub fn gen(&self, lang: Language, table_def: &TableDef) -> RS<String> {
        let builder: Arc<dyn SrcBuilder> = match lang {
            Language::Rust => Arc::new(RustBuilder::new())
        };
        let mut s = String::new();
        builder.build(table_def, &mut s)?;
        Ok(s)
    }
}

impl Language {
    pub fn lang_suffix(&self) -> &'static str {
        match self {
            Language::Rust => "rs",
        }
    }
}