use crate::ast::ast_node::ASTNode;
use mudu_type::dat_typed::DatTyped;

#[derive(Clone, Debug)]
pub enum ExprLiteral {
    Null,
    DatumLiteral(DatTyped),
}

impl ExprLiteral {
    pub fn dat_type(&self) -> &DatTyped {
        match self {
            ExprLiteral::Null => panic!("NULL literal has no concrete data type"),
            ExprLiteral::DatumLiteral(typed) => typed,
        }
    }
}

impl ASTNode for ExprLiteral {}

#[cfg(test)]
mod tests {
    use super::ExprLiteral;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dat_typed::DatTyped;

    #[test]
    fn expr_literal_returns_underlying_typed_value() {
        let literal = ExprLiteral::DatumLiteral(DatTyped::from_i32(11));
        assert_eq!(literal.dat_type().dat_type().dat_type_id(), DatTypeID::I32);
    }
}
