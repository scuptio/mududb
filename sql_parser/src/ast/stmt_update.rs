use crate::ast::ast_node::ASTNode;
use crate::ast::expr_compare::ExprCompare;
use crate::ast::expr_item::ExprValue;
use crate::ast::expression::ExprType;
#[derive(Clone, Debug)]
pub enum AssignedValue {
    Expression(ExprType),
    Value(ExprValue),
}

#[derive(Clone, Debug)]
pub struct Assignment {
    column_reference: String,
    value: AssignedValue,
}

#[derive(Clone, Debug)]
pub struct StmtUpdate {
    table_reference: String,
    set_values: Vec<Assignment>,
    where_predicate: Vec<ExprCompare>,
}

impl Assignment {
    pub fn new(column_reference: String, value: AssignedValue) -> Self {
        Self {
            column_reference,
            value,
        }
    }

    pub fn set_column_reference(&mut self, column_reference: String) {
        self.column_reference = column_reference
    }

    pub fn get_column_reference(&self) -> &String {
        &self.column_reference
    }

    pub fn set_set_value(&mut self, value: AssignedValue) {
        self.value = value
    }

    pub fn get_set_value(&self) -> &AssignedValue {
        &self.value
    }
}

impl StmtUpdate {
    pub fn new() -> Self {
        Self {
            table_reference: Default::default(),
            set_values: vec![],
            where_predicate: vec![],
        }
    }

    pub fn get_table_reference(&self) -> &String {
        &self.table_reference
    }
    pub fn set_table_reference(&mut self, name: String) {
        self.table_reference = name
    }

    pub fn get_where_predicate(&self) -> &Vec<ExprCompare> {
        &self.where_predicate
    }

    pub fn set_where_predicate(&mut self, pred_list: Vec<ExprCompare>) {
        self.where_predicate = pred_list
    }

    pub fn get_set_values(&self) -> &Vec<Assignment> {
        &self.set_values
    }

    pub fn set_set_values(&mut self, set_values: Vec<Assignment>) {
        self.set_values = set_values
    }
}

impl ASTNode for StmtUpdate {}
