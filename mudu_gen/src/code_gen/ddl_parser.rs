use crate::code_gen::table_def::TableDef;
use mudu::common::result::RS;
use sql_parser::ast::parser::SQLParser;
use sql_parser::ast::stmt_type::{StmtCommand, StmtType};

/// DDLParser
/// parser DDL SQL statement, and convert the Create Table SQL statement to a TableDef object,
/// other statement are ignored.
pub struct DDLParser {
    parser: SQLParser,
}

impl DDLParser {
    pub fn new() -> DDLParser {
        Self {
            parser: SQLParser::new(),
        }
    }

    /// parse SQL text and return a vector of TableDef
    pub fn parse(& self, text: &str) -> RS<Vec<TableDef>> {
        let stmt_list = self.parser.parse(text)?;
        let mut vec = vec![];
        for stmt in stmt_list.stmts() {
            if let StmtType::Command(StmtCommand::CreateTable(ddl)) = stmt {
                vec.push(TableDef::from_ddl(ddl)?);
            }
        }

        Ok(vec)
    }
}
