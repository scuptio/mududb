//! SQL statement abstraction.

use std::fmt;

/// An SQL statement that can be rendered to text and cloned behind a trait object.
pub trait SQLStmt: fmt::Debug + fmt::Display + Sync + Send {
    /// Returns the SQL text of the statement.
    fn to_sql_string(&self) -> String;

    /// Clones the statement into a boxed trait object.
    fn clone_boxed(&self) -> Box<dyn SQLStmt>;
}

/// Conversion to an [`SQLStmt`] trait object reference.
pub trait AsSQLStmtRef {
    /// Returns `self` as an [`SQLStmt`] trait object reference.
    fn as_sql_stmt_ref(&self) -> &dyn SQLStmt;
}
impl AsSQLStmtRef for Box<dyn SQLStmt> {
    fn as_sql_stmt_ref(&self) -> &dyn SQLStmt {
        self.as_ref()
    }
}

impl<U: AsSQLStmtRef + ?Sized> AsSQLStmtRef for &U {
    fn as_sql_stmt_ref(&self) -> &dyn SQLStmt {
        (*self).as_sql_stmt_ref()
    }
}

impl SQLStmt for &str {
    fn to_sql_string(&self) -> String {
        self.to_string()
    }

    fn clone_boxed(&self) -> Box<dyn SQLStmt> {
        Box::new(self.to_string().clone())
    }
}

impl SQLStmt for str {
    fn to_sql_string(&self) -> String {
        self.to_string()
    }

    fn clone_boxed(&self) -> Box<dyn SQLStmt> {
        Box::new(self.to_string().clone())
    }
}

impl SQLStmt for String {
    fn to_sql_string(&self) -> String {
        self.to_string()
    }

    fn clone_boxed(&self) -> Box<dyn SQLStmt> {
        Box::new(self.clone())
    }
}

impl<'a> AsRef<dyn SQLStmt + 'a> for String {
    fn as_ref(&self) -> &(dyn SQLStmt + 'a) {
        self
    }
}

impl<'a> AsRef<dyn SQLStmt + 'a> for &'a str {
    fn as_ref(&self) -> &(dyn SQLStmt + 'a) {
        self
    }
}
