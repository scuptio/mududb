use crate::ast::ast_node::ASTNode;
use crate::ast::expr_item::ExprValue;


#[derive(Debug, Clone)]
pub struct StmtInsert {
    table_reference: String,
    columns: Vec<String>,
    values_list: Vec<Vec<ExprValue>>
}

impl StmtInsert {
    pub fn new(
        table_reference: String,
        columns: Vec<String>,
        values_list: Vec<Vec<ExprValue>>,
    ) -> Self {
        Self {
            table_reference,
            columns,
            values_list,
        }
    }

    pub fn table_name(&self) -> &String {
        &self.table_reference
    }

    pub fn columns(&self) -> &Vec<String> {
        &self.columns
    }

    pub fn values_list(&self) -> &Vec<Vec<ExprValue>> {
        &self.values_list
    }
}

impl ASTNode for StmtInsert {}
