use crate::ast::ast_node::ASTNode;
use crate::ast::expr_compare::ExprCompare;
use crate::ast::object_ref::ObjectRef;

/// `DELETE` statement AST node.
#[derive(Clone, Debug)]
pub struct StmtDelete {
    table_reference: ObjectRef,
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
            table_reference: ObjectRef::default(),
            where_predicate: vec![],
        }
    }

    /// Return the target table reference (bare name plus optional schema qualifier).
    pub fn get_table_reference(&self) -> &ObjectRef {
        &self.table_reference
    }

    /// Set the target table reference.
    pub fn set_table_reference(&mut self, table: ObjectRef) {
        self.table_reference = table
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
