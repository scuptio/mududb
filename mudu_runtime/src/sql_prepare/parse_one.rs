use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use sql_parser::ast::parser::SQLParser;
use sql_parser::ast::stmt_select::StmtSelect;
use sql_parser::ast::stmt_type::{StmtCommand, StmtType};

pub fn parse_one_query(parser: &SQLParser, sql: &String) -> RS<StmtSelect> {
    let stmt_list = parser.parse(sql)?;
    if stmt_list.stmts().len() != 1 {
        return Err(m_error!(EC::ParseErr, "SQL text must be one select statement"));
    }
    let select_stmt = stmt_list.into_stmts().pop().unwrap();
    match select_stmt {
        StmtType::Select(select) => {
            Ok(select)
        }
        _ => Err(m_error!(EC::ParseErr, "SQL must be select statement")),
    }
}


pub fn parse_one_command(parser: &SQLParser,  sql: &String) -> RS<StmtCommand> {
    let stmt_list = parser.parse(sql)?;
    if stmt_list.stmts().len() != 1 {
        return Err(m_error!(EC::ParseErr, "SQL text must be one select statement"));
    }
    let stmt_command = stmt_list.into_stmts().pop().unwrap();
    match stmt_command {
        StmtType::Command(command) => {
            Ok(command)
        }
        _ => Err(m_error!(EC::ParseErr, "SQL must be command statement")),
    }
}