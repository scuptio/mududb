//! `database::sql_stmt_text` module.
#![allow(missing_docs)]

use mudu_type::dat_type::DatType;
use std::fmt::{Debug, Display, Formatter};

use crate::database::sql_stmt::SQLStmt;

#[derive(Clone)]
pub struct SQLStmtText {
    text: String,
}

#[allow(unused)]
impl SQLStmtText {
    pub fn into(self) -> String {
        self.text
    }

    pub fn new(text: String) -> SQLStmtText {
        Self { text }
    }

    pub fn param_ty(&self) -> &[DatType] {
        // Placeholder: placeholder type parsing is not yet implemented.
        &[]
    }
}

impl Debug for SQLStmtText {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.text, f)
    }
}

impl Display for SQLStmtText {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.text, f)
    }
}

impl SQLStmt for SQLStmtText {
    fn to_sql_string(&self) -> String {
        self.text.clone()
    }

    fn clone_boxed(&self) -> Box<dyn SQLStmt> {
        Box::new(self.clone())
    }
}
