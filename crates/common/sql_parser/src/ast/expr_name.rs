use crate::ast::ast_node::ASTNode;

/// Named identifier expression (table, column, or alias name).
#[derive(Clone, Debug)]
pub struct ExprName {
    name: String,
}

impl ExprName {
    /// Create a new empty name expression.
    pub fn new() -> Self {
        Self {
            name: "".to_string(),
        }
    }

    /// Set the identifier name.
    pub fn set_name(&mut self, name: String) {
        self.name = name
    }

    /// Return the identifier name.
    pub fn name(&self) -> &String {
        &self.name
    }
}

impl Default for ExprName {
    fn default() -> Self {
        Self::new()
    }
}

impl ASTNode for ExprName {}
