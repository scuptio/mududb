use crate::ast::expr_name::ExprName;

/// A single term in a `SELECT` list, optionally with an alias.
#[derive(Clone, Debug)]
pub struct SelectTerm {
    field: ExprName,
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
            field: ExprName::new(),
            alias: Default::default(),
        }
    }

    /// Set the selected field expression.
    pub fn set_field(&mut self, field: ExprName) {
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
    pub fn field(&self) -> &ExprName {
        &self.field
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]

    use super::*;

    #[test]
    fn default_creates_empty_select_term() {
        let term = SelectTerm::default();
        assert!(term.field().name().is_empty());
        assert!(term.alias().is_empty());
    }

    #[test]
    fn new_creates_empty_field_and_alias() {
        let term = SelectTerm::new();
        assert!(term.field().name().is_empty());
        assert!(term.alias().is_empty());
    }

    #[test]
    fn set_field_updates_field() {
        let mut term = SelectTerm::new();
        let mut field = ExprName::new();
        field.set_name("col".to_string());
        term.set_field(field);
        assert_eq!(term.field().name(), "col");
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
        let mut field = ExprName::new();
        field.set_name("col".to_string());
        term.set_field(field);
        term.set_alias("alias".to_string());
        let cloned = term.clone();
        assert_eq!(cloned.field().name(), "col");
        assert_eq!(cloned.alias(), "alias");
    }

    #[test]
    fn debug_format_contains_field_and_alias() {
        let mut term = SelectTerm::new();
        let mut field = ExprName::new();
        field.set_name("col".to_string());
        term.set_field(field);
        term.set_alias("alias".to_string());
        let debug = format!("{:?}", term);
        assert!(debug.contains("col"));
        assert!(debug.contains("alias"));
    }
}
