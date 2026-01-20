use mudu::common::result::RS;
use mudu_binding::record::field_def::FieldDef;
use mudu_binding::record::record_def::RecordDef;
use crate::ast::parser::SQLParser;
use crate::ast::stmt_create_table::StmtCreateTable;
use crate::ast::stmt_type::{StmtCommand, StmtType};

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
    pub fn parse(&self, text: &str) -> RS<Vec<RecordDef>> {
        let stmt_list = self.parser.parse(text)?;
        let mut vec = vec![];
        for stmt in stmt_list.stmts() {
            if let StmtType::Command(StmtCommand::CreateTable(ddl)) = stmt {
                vec.push(Self::record_def(ddl)?);
            }
        }

        Ok(vec)
    }

    fn record_def(stmt: &StmtCreateTable) -> RS<RecordDef> {
        let column_def_vec = stmt
            .column_def()
            .iter()
            .map(|d| {
                let column_def = FieldDef::new(
                    d.column_name().clone(),
                    d.data_type().clone(),
                    d.is_primary_key(),
                );
                column_def
            })
            .collect();

        let table_def = RecordDef::new(
            stmt.table_name().clone(),
            column_def_vec,
        );
        Ok(table_def)
    }
}

