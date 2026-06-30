//! Literal expression AST node.

use crate::ast::ast_node::ASTNode;
use mudu_type::dat_typed::DatTyped;

/// Literal value expression.
#[derive(Clone, Debug)]
pub enum ExprLiteral {
    /// SQL `NULL` literal.
    Null,
    /// Typed datum literal (e.g., integer, string, numeric).
    DatumLiteral(DatTyped),
}

impl ExprLiteral {
    /// Returns the concrete data type of the literal, if any.
    ///
    /// `NULL` literals do not have a concrete data type and return `None`.
    pub fn dat_type(&self) -> Option<&DatTyped> {
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
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dat_typed::DatTyped;

    #[test]
    fn expr_literal_returns_underlying_typed_value() {
        let literal = ExprLiteral::DatumLiteral(DatTyped::from_i32(11));
        assert_eq!(
            literal.dat_type().unwrap().dat_type().dat_type_id(),
            DatTypeID::I32
        );
    }
}
