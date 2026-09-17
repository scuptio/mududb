//! Diagnostic types produced by the SQL checker.

use std::fmt;

/// Severity of a single SQL diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqlSeverity {
    /// The statement is invalid: a syntax error, an unknown table or column,
    /// or another problem that would also fail at runtime.
    Error,
    /// The statement uses valid SQL constructs that the supported subset does
    /// not cover (for example `JOIN` or subqueries). It cannot be checked
    /// further, but it is not an error.
    Uncovered,
}

impl SqlSeverity {
    /// Lower-case severity label used when rendering diagnostics.
    pub fn label(&self) -> &'static str {
        match self {
            SqlSeverity::Error => "error",
            SqlSeverity::Uncovered => "uncovered",
        }
    }
}

impl fmt::Display for SqlSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// A single diagnostic produced for a SQL statement.
///
/// `line` and `column` are 1-based positions relative to the start of the
/// checked SQL text; front-ends that extract SQL from source files add the
/// literal's own position to report file locations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqlDiagnostic {
    severity: SqlSeverity,
    message: String,
    line: usize,
    column: usize,
}

impl SqlDiagnostic {
    /// Create a new diagnostic.
    pub fn new(severity: SqlSeverity, message: String, line: usize, column: usize) -> Self {
        Self {
            severity,
            message,
            line,
            column,
        }
    }

    /// Return the severity.
    pub fn severity(&self) -> SqlSeverity {
        self.severity
    }

    /// Return the human-readable message.
    pub fn message(&self) -> &String {
        &self.message
    }

    /// Return the 1-based line within the checked SQL text.
    pub fn line(&self) -> usize {
        self.line
    }

    /// Return the 1-based column within the checked SQL text.
    pub fn column(&self) -> usize {
        self.column
    }
}
