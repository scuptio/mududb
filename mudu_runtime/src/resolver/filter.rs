use crate::resolver::item_value::ItemValue;
use sql_parser::ast::expr_operator::ValueCompare;

pub struct Filter {
    value_compare: ValueCompare,
    filter_value: ItemValue,
}

impl Filter {
    pub fn new(value_compare: ValueCompare, filter_value: ItemValue) -> Self {
        Self {
            value_compare,
            filter_value,
        }
    }

    pub fn compare_op(&self) -> ValueCompare {
        self.value_compare
    }

    pub fn filter_value(&self) -> &ItemValue {
        &self.filter_value
    }
}