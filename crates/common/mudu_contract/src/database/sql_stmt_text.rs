//! SQL statement text wrapper.

use mudu_type::data_type::DataType;
use std::fmt::{Debug, Display, Formatter};

use crate::database::sql_stmt::SQLStmt;

/// An SQL statement carried as plain text.
#[derive(Clone)]
pub struct SQLStmtText {
    text: String,
}

#[allow(unused)]
impl SQLStmtText {
    /// Consumes `self` and returns the SQL text.
    pub fn into(self) -> String {
        self.text
    }

    /// Creates a statement from SQL text.
    pub fn new(text: String) -> SQLStmtText {
        Self { text }
    }

    /// Returns the types of the statement's placeholders.
    pub fn param_ty(&self) -> &[DataType] {
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
