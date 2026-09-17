use crate::ast::ast_node::ASTNode;
use crate::ast::expr_name::ExprName;

/// Argument of a function call in a select list.
#[derive(Clone, Debug)]
pub enum FunctionArg {
    /// The `*` argument, e.g. `COUNT(*)`.
    Star,
    /// A single column reference argument, e.g. `SUM(col)`.
    Column(ExprName),
}

/// Function call expression in a select list, e.g. `COUNT(*)` or `SUM(col)`.
#[derive(Clone, Debug)]
pub struct ExprFunction {
    name: String,
    arg: FunctionArg,
}

impl ExprFunction {
    /// Create a function call expression with the given name and argument.
    pub fn new(name: String, arg: FunctionArg) -> Self {
        Self { name, arg }
    }

    /// Return the function name as written in the SQL text.
    pub fn name(&self) -> &String {
        &self.name
    }

    /// Return the function argument.
    pub fn arg(&self) -> &FunctionArg {
        &self.arg
    }
}

impl ASTNode for ExprFunction {}
