use crate::ast::expr_name::ExprName;

#[derive(Clone, Debug)]
pub struct SelectTerm {
    field: ExprName,
    alias: String,
}

impl SelectTerm {
    pub fn new() -> Self {
        Self {
            field: ExprName::new(),
            alias: Default::default(),
        }
    }

    pub fn set_field(&mut self, field: ExprName) {
        self.field = field
    }

    pub fn set_alias(&mut self, alias: String) {
        self.alias = alias;
    }

    pub fn alias(&self) -> &String {
        &self.alias
    }

    pub fn field(&self) -> &ExprName {
        &self.field
    }
}
