use crate::ast::ast_node::ASTNode;
use crate::ast::expr_compare::ExprCompare;
use crate::ast::select_term::SelectTerm;
use std::fmt::Debug;

/// `SELECT` statement AST node.
#[derive(Clone, Debug)]
pub struct StmtSelect {
    select_term_list: Vec<SelectTerm>,
    table_reference: String,
    // currently, we only support and logical connective expression
    where_predicate: Vec<ExprCompare>,
}

impl Default for StmtSelect {
    fn default() -> Self {
        Self::new()
    }
}

impl StmtSelect {
    /// Create a new empty `SELECT` statement.
    pub fn new() -> Self {
        Self {
            select_term_list: vec![],
            table_reference: "".to_string(),
            where_predicate: vec![],
        }
    }

    /// Add a term to the `SELECT` list.
    pub fn add_select_term(&mut self, select_term: SelectTerm) {
        self.select_term_list.push(select_term);
    }

    /// Add a predicate to the `WHERE` clause.
    pub fn add_where_predicate(&mut self, pred: ExprCompare) {
        self.where_predicate.push(pred);
    }

    /// Return all `WHERE` predicates.
    pub fn get_where_predicate(&self) -> &Vec<ExprCompare> {
        &self.where_predicate
    }

    /// Return the `SELECT` list terms.
    pub fn get_select_term_list(&self) -> &Vec<SelectTerm> {
        &self.select_term_list
    }

    /// Set the table reference (`FROM` clause).
    pub fn set_table_reference(&mut self, table: String) {
        self.table_reference = table;
    }

    /// Return the table reference.
    pub fn get_table_reference(&self) -> &String {
        &self.table_reference
    }
}

impl ASTNode for StmtSelect {}
