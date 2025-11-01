use crate::ast::ast_node::ASTNode;
use mudu::data_type::dt_impl::dat_typed::DatTyped;

#[derive(Clone, Debug)]
pub enum ExprLiteral {
    DatumLiteral(DatTyped),
}

impl ExprLiteral {
    pub fn dat_type(&self) -> &DatTyped {
        match self {
            ExprLiteral::DatumLiteral(typed) => typed,
        }
    }
}

impl ASTNode for ExprLiteral {}
