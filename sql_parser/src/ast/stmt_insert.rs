use crate::ast::ast_node::ASTNode;
use crate::ast::expr_item::ExprValue;

/// `INSERT` statement AST node.
#[derive(Debug, Clone)]
pub struct StmtInsert {
    table_reference: String,
    columns: Vec<String>,
    values_list: Vec<Vec<ExprValue>>,
}

impl StmtInsert {
    /// Create a new `INSERT` statement.
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

    /// Return the target table name.
    pub fn table_name(&self) -> &String {
        &self.table_reference
    }

    /// Return the target column names.
    pub fn columns(&self) -> &Vec<String> {
        &self.columns
    }

    /// Return the inserted values (one vector per row).
    pub fn values_list(&self) -> &Vec<Vec<ExprValue>> {
        &self.values_list
    }
}

impl ASTNode for StmtInsert {}
