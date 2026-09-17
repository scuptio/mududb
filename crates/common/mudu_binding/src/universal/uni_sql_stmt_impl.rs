use crate::universal::uni_sql_stmt::UniSqlStmt;
use mudu::common::result::RS;
use mudu_contract::database::sql_stmt_text::SQLStmtText;

impl UniSqlStmt {
    pub fn uni_to(self) -> RS<SQLStmtText> {
        Ok(SQLStmtText::new(self.sql_string))
    }

    pub fn uni_from(s: SQLStmtText) -> RS<UniSqlStmt> {
        Ok(Self {
            sql_string: s.into(),
        })
    }
}
