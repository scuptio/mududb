use crate::ast::ast_node::ASTNode;
use crate::ast::expr_compare::ExprCompare;

/// `DELETE` statement AST node.
#[derive(Clone, Debug)]
pub struct StmtDelete {
    table_reference: String,
    where_predicate: Vec<ExprCompare>,
}

impl Default for StmtDelete {
    fn default() -> Self {
        Self::new()
    }
}

impl StmtDelete {
    /// Create a new empty `DELETE` statement.
    pub fn new() -> Self {
        Self {
            table_reference: "".to_string(),
            where_predicate: vec![],
        }
    }

    /// Return the target table name.
    pub fn get_table_reference(&self) -> &String {
        &self.table_reference
    }

    /// Set the target table name.
    pub fn set_table_reference(&mut self, name: String) {
        self.table_reference = name
    }

    /// Add a `WHERE` predicate.
    pub fn add_where_predicate(&mut self, pred: ExprCompare) {
        self.where_predicate.push(pred);
    }

    /// Return all `WHERE` predicates.
    pub fn get_where_predicate(&self) -> &Vec<ExprCompare> {
        &self.where_predicate
    }

    /// Replace all `WHERE` predicates.
    pub fn set_where_predicate(&mut self, where_predicate: Vec<ExprCompare>) {
        self.where_predicate = where_predicate;
    }
}

impl ASTNode for StmtDelete {}
