use crate::code_gen::table_def::TableDef;
use mudu::common::result::RS;
use std::fmt::Write;
pub trait SrcBuilder {
    fn build(&self, table_def: &TableDef, writer: &mut dyn Write) -> RS<()>;
}
