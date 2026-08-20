use crate::ast::expr_function::ExprFunction;
use crate::ast::expr_name::ExprName;

/// What a single `SELECT` list term selects: a plain column or a function call.
#[derive(Clone, Debug)]
pub enum SelectField {
    /// A plain column reference (an empty name represents `*` / all fields).
    Column(ExprName),
    /// A function call, e.g. `COUNT(*)` or `SUM(col)`.
    Function(ExprFunction),
}

/// A single term in a `SELECT` list, optionally with an alias.
#[derive(Clone, Debug)]
pub struct SelectTerm {
    field: SelectField,
    alias: String,
}

impl Default for SelectTerm {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectTerm {
    /// Create a new select term with an empty field and alias.
    pub fn new() -> Self {
        Self {
            field: SelectField::Column(ExprName::new()),
            alias: Default::default(),
        }
    }

    /// Set the selected field expression.
    pub fn set_field(&mut self, field: SelectField) {
        self.field = field
    }

    /// Set the alias for this select term.
    pub fn set_alias(&mut self, alias: String) {
        self.alias = alias;
    }

    /// Return the alias, if any.
    pub fn alias(&self) -> &String {
        &self.alias
    }

    /// Return the selected field expression.
    pub fn field(&self) -> &SelectField {
        &self.field
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]

    use super::*;
    use crate::ast::expr_function::FunctionArg;

    fn column_field(name: &str) -> SelectField {
        let mut field = ExprName::new();
        field.set_name(name.to_string());
        SelectField::Column(field)
    }

    fn column_name(field: &SelectField) -> &str {
        match field {
            SelectField::Column(name) => name.name(),
            SelectField::Function(_) => panic!("expected column field"),
        }
    }

    #[test]
    fn default_creates_empty_select_term() {
        let term = SelectTerm::default();
        assert!(column_name(term.field()).is_empty());
        assert!(term.alias().is_empty());
    }

    #[test]
    fn new_creates_empty_field_and_alias() {
        let term = SelectTerm::new();
        assert!(column_name(term.field()).is_empty());
        assert!(term.alias().is_empty());
    }

    #[test]
    fn set_field_updates_field() {
        let mut term = SelectTerm::new();
        term.set_field(column_field("col"));
        assert_eq!(column_name(term.field()), "col");
    }

    #[test]
    fn set_function_field() {
        let mut term = SelectTerm::new();
        term.set_field(SelectField::Function(ExprFunction::new(
            "count".to_string(),
            FunctionArg::Star,
        )));
        match term.field() {
            SelectField::Function(f) => {
                assert_eq!(f.name(), "count");
                assert!(matches!(f.arg(), FunctionArg::Star));
            }
            SelectField::Column(_) => panic!("expected function field"),
        }
    }

    #[test]
    fn set_alias_updates_alias() {
        let mut term = SelectTerm::new();
        term.set_alias("alias".to_string());
        assert_eq!(term.alias(), "alias");
    }

    #[test]
    fn clone_preserves_field_and_alias() {
        let mut term = SelectTerm::new();
        term.set_field(column_field("col"));
        term.set_alias("alias".to_string());
        let cloned = term.clone();
        assert_eq!(column_name(cloned.field()), "col");
        assert_eq!(cloned.alias(), "alias");
    }

    #[test]
    fn debug_format_contains_field_and_alias() {
        let mut term = SelectTerm::new();
        term.set_field(column_field("col"));
        term.set_alias("alias".to_string());
        let debug = format!("{:?}", term);
        assert!(debug.contains("col"));
        assert!(debug.contains("alias"));
    }
}
