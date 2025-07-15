use crate::ast::ast_node::ASTNode;

#[derive(Clone, Debug)]
pub struct ExprName {
    name: String,
}

impl ExprName {
    pub fn new() -> Self {
        Self {
            name: "".to_string(),
        }
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name
    }

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
