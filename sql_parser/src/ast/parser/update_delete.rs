use super::context::ParseContext;
use super::SQLParser;
use crate::ast::expr_item::ExprItem;
use crate::ast::expression::ExprType;
use crate::ast::stmt_delete::StmtDelete;
use crate::ast::stmt_update::{AssignedValue, Assignment, StmtUpdate};
use crate::ts_const::ts_field_name;
use mudu::common::result::RS;
use mudu::common::result_of::rs_option;
use tree_sitter::Node;

impl SQLParser {
    pub(crate) fn visit_update_statement(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<StmtUpdate> {
        let mut stmt = StmtUpdate::new();

        let opt = node.child_by_field_name(ts_field_name::OBJECT_REFERENCE);
        let n_object_reference = rs_option(opt, "")?;
        let table_reference = self.visit_object_reference(context, n_object_reference)?;
        stmt.set_table_reference(table_reference);

        let opt = node.child_by_field_name(ts_field_name::SET_VALUES);
        let n_set_values = rs_option(opt, "no set values clause in update statement")?;
        let set_values = self.visit_set_values(context, n_set_values)?;
        stmt.set_set_values(set_values);

        let opt = node.child_by_field_name(ts_field_name::WHERE);
        let n_where = rs_option(opt, "no where clause in update statement")?;
        let expr_list = self.visit_where(context, n_where)?;
        stmt.set_where_predicate(expr_list);

        Ok(stmt)
    }

    pub(crate) fn visit_delete_statement(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<StmtDelete> {
        let mut stmt = StmtDelete::new();
        let opt = node.child_by_field_name(ts_field_name::OBJECT_REFERENCE);
        let n_object_reference = rs_option(opt, "no object reference in delete statement")?;
        let table_reference = self.visit_object_reference(context, n_object_reference)?;
        stmt.set_table_reference(table_reference);
        let opt = node.child_by_field_name(ts_field_name::WHERE);
        let n_where = rs_option(opt, "no where clause in delete statement")?;
        let expr_list = self.visit_where(context, n_where)?;
        stmt.set_where_predicate(expr_list);
        Ok(stmt)
    }

    pub(crate) fn visit_set_values(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<Vec<Assignment>> {
        let mut cursor = node.walk();
        let mut set_values = vec![];
        let iter = node.children_by_field_name(ts_field_name::ASSIGNMENT, &mut cursor);
        for n in iter {
            let assignment = self.visit_assignment(context, n)?;
            set_values.push(assignment);
        }
        Ok(set_values)
    }

    pub(crate) fn visit_assignment(&self, context: &ParseContext, node: Node) -> RS<Assignment> {
        let opt = node.child_by_field_name(ts_field_name::LEFT);
        let n_left = rs_option(opt, "no left in assignment node")?;
        let column_reference = self.visit_field(context, n_left)?;

        let opt = node.child_by_field_name(ts_field_name::RIGHT);
        let n_right = rs_option(opt, "no right in assignment node")?;
        let expr = self.visit_expression(context, n_right)?;
        let expr_l = match &expr {
            ExprType::Value(value) => match &(**value) {
                ExprItem::ItemValue(value) => AssignedValue::Value(value.clone()),
                _ => AssignedValue::Expression(expr),
            },
            _ => AssignedValue::Expression(expr),
        };

        let assignment = Assignment::new(column_reference, expr_l);
        Ok(assignment)
    }

    pub(crate) fn visit_field(&self, context: &ParseContext, node: Node) -> RS<String> {
        super::error::ts_node_context_string(context.parse_str(), &node)
    }
}
