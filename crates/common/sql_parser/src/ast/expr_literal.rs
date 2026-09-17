//! Literal expression AST node.

use crate::ast::ast_node::ASTNode;
use mudu_type::data_typed::DataTyped;

/// Literal value expression.
#[derive(Clone, Debug)]
pub enum ExprLiteral {
    /// SQL `NULL` literal.
    Null,
    /// Typed datum literal (e.g., integer, string, numeric).
    DatumLiteral(DataTyped),
}

impl ExprLiteral {
    /// Returns the concrete data type of the literal, if any.
    ///
    /// `NULL` literals do not have a concrete data type and return `None`.
    pub fn data_type(&self) -> Option<&DataTyped> {
        match self {
            ExprLiteral::Null => None,
            ExprLiteral::DatumLiteral(typed) => Some(typed),
        }
    }
}

impl ASTNode for ExprLiteral {}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]

    use super::ExprLiteral;
    use mudu_type::data_typed::DataTyped;
    use mudu_type::type_family::TypeFamily;

    #[test]
    fn expr_literal_returns_underlying_typed_value() {
        let literal = ExprLiteral::DatumLiteral(DataTyped::from_i32(11));
        assert_eq!(
            literal.data_type().unwrap().data_type().type_family(),
            TypeFamily::I32
        );
    }
}
