use crate::ast::ast_node::ASTNode;
use crate::ast::expr_compare::ExprCompare;
use crate::ast::expr_item::ExprValue;
use crate::ast::expression::ExprType;

/// Value assigned in an `UPDATE` `SET` clause.
#[derive(Clone, Debug)]
pub enum AssignedValue {
    /// Arbitrary expression value.
    Expression(ExprType),
    /// Single scalar value or placeholder.
    Value(ExprValue),
}

/// Single `SET column = value` assignment in an `UPDATE` statement.
#[derive(Clone, Debug)]
pub struct Assignment {
    column_reference: String,
    value: AssignedValue,
}

/// `UPDATE` statement AST node.
#[derive(Clone, Debug)]
pub struct StmtUpdate {
    table_reference: String,
    set_values: Vec<Assignment>,
    where_predicate: Vec<ExprCompare>,
}

impl Assignment {
    /// Create a new assignment.
    pub fn new(column_reference: String, value: AssignedValue) -> Self {
        Self {
            column_reference,
            value,
        }
    }

    /// Set the target column reference.
    pub fn set_column_reference(&mut self, column_reference: String) {
        self.column_reference = column_reference
    }

    /// Return the target column reference.
    pub fn get_column_reference(&self) -> &String {
        &self.column_reference
    }

    /// Set the assigned value.
    pub fn set_set_value(&mut self, value: AssignedValue) {
        self.value = value
    }

    /// Return the assigned value.
    pub fn get_set_value(&self) -> &AssignedValue {
        &self.value
    }
}

impl Default for StmtUpdate {
    fn default() -> Self {
        Self::new()
    }
}

impl StmtUpdate {
    /// Create a new empty `UPDATE` statement.
    pub fn new() -> Self {
        Self {
            table_reference: Default::default(),
            set_values: vec![],
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

    /// Return all `WHERE` predicates.
    pub fn get_where_predicate(&self) -> &Vec<ExprCompare> {
        &self.where_predicate
    }

    /// Replace all `WHERE` predicates.
    pub fn set_where_predicate(&mut self, pred_list: Vec<ExprCompare>) {
        self.where_predicate = pred_list
    }

    /// Return all `SET` assignments.
    pub fn get_set_values(&self) -> &Vec<Assignment> {
        &self.set_values
    }

    /// Replace all `SET` assignments.
    pub fn set_set_values(&mut self, set_values: Vec<Assignment>) {
        self.set_values = set_values
    }
}

impl ASTNode for StmtUpdate {}
