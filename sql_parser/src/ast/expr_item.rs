//! Expression item AST node.

use crate::ast::expr_literal::ExprLiteral;
use crate::ast::expr_name::ExprName;
use std::fmt::Debug;

/// Atomic expression item: a name or a value.
#[derive(Clone, Debug)]
pub enum ExprItem {
    /// Named identifier (table/column reference).
    ItemName(ExprName),
    /// Value (literal or placeholder).
    ItemValue(ExprValue),
}

/// Expression value: a literal or a placeholder.
#[derive(Clone, Debug)]
pub enum ExprValue {
    /// Typed literal value.
    ValueLiteral(ExprLiteral),
    /// Parameter placeholder (`?`).
    ValuePlaceholder,
}

impl ExprItem {
    /// If this item is a named field, return the name.
    pub fn to_field(&self) -> Option<&ExprName> {
        if let ExprItem::ItemName(field) = self {
            Some(field)
        } else {
            None
        }
    }

    /// If this item is a literal value, return the literal.
    pub fn to_literal(&self) -> Option<&ExprLiteral> {
        if let ExprItem::ItemValue(ExprValue::ValueLiteral(literal)) = self {
            Some(literal)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]

    use super::{ExprItem, ExprValue};
    use crate::ast::expr_literal::ExprLiteral;
    use crate::ast::expr_name::ExprName;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dat_typed::DatTyped;

    #[test]
    fn expr_item_to_field_returns_name_only_for_name_variant() {
        let mut field = ExprName::new();
        field.set_name("id".to_string());
        let name = ExprItem::ItemName(field);
        assert_eq!(name.to_field().unwrap().name(), "id");

        let literal = ExprItem::ItemValue(ExprValue::ValueLiteral(ExprLiteral::DatumLiteral(
            DatTyped::from_i32(7),
        )));
        assert!(literal.to_field().is_none());
    }

    #[test]
    fn expr_item_to_literal_returns_literal_only_for_literal_variant() {
        let literal = ExprItem::ItemValue(ExprValue::ValueLiteral(ExprLiteral::DatumLiteral(
            DatTyped::from_string("alice".to_string()),
        )));
        assert_eq!(
            literal
                .to_literal()
                .unwrap()
                .dat_type()
                .unwrap()
                .dat_type()
                .dat_type_id(),
            DatTypeID::String
        );

        let placeholder = ExprItem::ItemValue(ExprValue::ValuePlaceholder);
        assert!(placeholder.to_literal().is_none());
    }
}
