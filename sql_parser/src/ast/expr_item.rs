use crate::ast::expr_literal::ExprLiteral;
use crate::ast::expr_name::ExprName;
use std::fmt::Debug;

#[derive(Clone, Debug)]
pub enum ExprItem {
    ItemName(ExprName),
    ItemValue(ExprValue),
}

#[derive(Clone, Debug)]
pub enum ExprValue {
    ValueLiteral(ExprLiteral),
    ValuePlaceholder
}

impl ExprItem {
    pub fn to_field(&self) -> Option<&ExprName> {
        if let ExprItem::ItemName(field) = self {
            Some(field)
        } else {
            None
        }
    }

    pub fn to_literal(&self) -> Option<&ExprLiteral> {
        if let ExprItem::ItemValue(ExprValue::ValueLiteral(literal)) = self {
            Some(literal)
        } else {
            None
        }
    }
}
