use crate::ast::ast_node::ASTNode;
use crate::ast::column_def::ColumnDef;

/// `CREATE TYPE <name> AS (<col> <type>, ...)` statement AST node.
///
/// A transient type declaration without storage: it declares an object shape
/// (fields/types) that projects to a WIT `record` but has no backing table.
#[derive(Clone, Debug)]
pub struct StmtCreateType {
    name: String,
    columns: Vec<ColumnDef>,
}

impl StmtCreateType {
    /// Create a new `CREATE TYPE` statement from a name and column list.
    pub fn new(name: String, columns: Vec<ColumnDef>) -> Self {
        Self { name, columns }
    }

    /// Return the type name.
    pub fn name(&self) -> &String {
        &self.name
    }

    /// Return the column definitions.
    pub fn columns(&self) -> &Vec<ColumnDef> {
        &self.columns
    }
}

impl ASTNode for StmtCreateType {}
