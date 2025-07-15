use crate::ast::ast_node::ASTNode;
use crate::ast::expr_compare::ExprCompare;


#[derive(Clone, Debug)]
pub struct StmtDelete {
    table_reference: String,
    where_predicate: Vec<ExprCompare>,
}

impl StmtDelete {
    pub fn new() -> Self {
        Self {
            table_reference: "".to_string(),
            where_predicate: vec![],
        }
    }

    pub fn get_table_reference(&self) -> &String {
        &self.table_reference
    }
    pub fn set_table_reference(&mut self, name: String) {
        self.table_reference = name
    }

    pub fn add_where_predicate(&mut self, pred: ExprCompare) {
        self.where_predicate.push(pred);
    }

    pub fn get_where_predicate(&self) -> &Vec<ExprCompare> {
        &self.where_predicate
    }

    pub fn set_where_predicate(&mut self, where_predicate: Vec<ExprCompare>) {
        self.where_predicate = where_predicate;
    }
}

impl ASTNode for StmtDelete {}
