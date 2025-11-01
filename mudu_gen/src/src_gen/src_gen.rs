use crate::src_gen::rust_builder::RustBuilder;
use crate::src_gen::src_builder::SrcBuilder;
use crate::src_gen::table_def::TableDef;
use clap::ValueEnum;
use mudu::common::result::RS;
use rust_format::{Formatter, RustFmt};
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

    pub fn generate(&self, lang: Language, table_def: &TableDef) -> RS<String> {
        let builder: Arc<dyn SrcBuilder> = match lang {
            Language::Rust => Arc::new(RustBuilder::new()),
        };
        let mut s = String::new();
        builder.build(table_def, &mut s)?;
        let formatted = RustFmt::default().format_str(s).unwrap();
        Ok(formatted)
    }
}

impl Language {
    pub fn lang_suffix(&self) -> &'static str {
        match self {
            Language::Rust => "rs",
        }
    }
}
