//! SELECT statement parser.

use super::context::ParseContext;
use super::error::ts_node_context_string;
use super::SQLParser;
use crate::ast::expr_compare::ExprCompare;
use crate::ast::expr_function::{ExprFunction, FunctionArg};
use crate::ast::expr_visitor::ExprVisitor;
use crate::ast::select_term::{SelectField, SelectTerm};
use crate::ast::stmt_select::StmtSelect;
use crate::ts_const::ts_field_name;
use mudu::common::result::RS;
use mudu::common::result_of::rs_option;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tree_sitter::Node;

impl SQLParser {
    pub(crate) fn visit_select_statement(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<StmtSelect> {
        let mut stmt = StmtSelect::new();
        let opt_select = node.child_by_field_name(ts_field_name::SELECT);
        let select = match opt_select {
            Some(select) => select,
            None => {
                return Err(mudu_error!(ErrorCode::Parse, "no select statement"));
            }
        };
        let opt_from = node.child_by_field_name(ts_field_name::FROM);
        let from = match opt_from {
            Some(from) => from,
            None => {
                return Err(mudu_error!(ErrorCode::Parse, "no from field"));
            }
        };

        self.visit_select(context, select, &mut stmt)?;
        self.visit_from(context, from, &mut stmt)?;
        Ok(stmt)
    }

    pub(crate) fn visit_select(
        &self,
        context: &ParseContext,
        node: Node,
        stmt: &mut StmtSelect,
    ) -> RS<()> {
        let opt_select_expression = node.child_by_field_name(ts_field_name::SELECT_EXPRESSION);
        let select_expression = match opt_select_expression {
            Some(e) => e,
            None => {
                return Err(mudu_error!(ErrorCode::Parse, "no select expression"));
            }
        };

        self.visit_select_expression(context, select_expression, stmt)?;

        Ok(())
    }

    pub(crate) fn visit_from(
        &self,
        context: &ParseContext,
        node: Node,
        stmt: &mut StmtSelect,
    ) -> RS<()> {
        let opt_n_relation = node.child_by_field_name(ts_field_name::RELATION);
        let n_relation = rs_option(opt_n_relation, "")?;
        self.visit_relation(context, n_relation, stmt)?;
        let opt_n_where = node.child_by_field_name(ts_field_name::WHERE);
        if let Some(n_where) = opt_n_where {
            let where_predicate_list = self.visit_where(context, n_where)?;
            for p in where_predicate_list {
                stmt.add_where_predicate(p);
            }
        }

        Ok(())
    }

    pub(crate) fn visit_where(&self, context: &ParseContext, node: Node) -> RS<Vec<ExprCompare>> {
        let opt = node.child_by_field_name(ts_field_name::PREDICATE);
        let n_predicate = rs_option(opt, "")?;
        let where_predicate_list = self.visit_where_predicate_expression(context, n_predicate)?;
        Ok(where_predicate_list)
    }

    pub(crate) fn visit_where_predicate_expression(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<Vec<ExprCompare>> {
        let expr = self.visit_expression(context, node)?;
        let mut cmp_list = vec![];
        ExprVisitor::extract_expr_compare_list(expr, &mut cmp_list)?;
        Ok(cmp_list)
    }

    pub(crate) fn visit_relation(
        &self,
        context: &ParseContext,
        node: Node,
        stmt: &mut StmtSelect,
    ) -> RS<()> {
        let opt_n_object_reference = node.child_by_field_name(ts_field_name::OBJECT_REFERENCE);
        let n_object_reference =
            rs_option(opt_n_object_reference, "no object reference in relation")?;
        let name = self.visit_object_reference(context, n_object_reference)?;
        stmt.set_table_reference(name);
        Ok(())
    }

    pub(crate) fn visit_object_reference(&self, context: &ParseContext, node: Node) -> RS<String> {
        let opt_n_object_name = node.child_by_field_name(ts_field_name::OBJECT_NAME);
        let n_object_name = rs_option(opt_n_object_name, "no object name in object reference")?;
        let name = ts_node_context_string(context.parse_str(), &n_object_name)?;
        Ok(name)
    }

    pub(crate) fn visit_select_expression(
        &self,
        context: &ParseContext,
        node: Node,
        stmt: &mut StmtSelect,
    ) -> RS<()> {
        for i in 0..node.child_count() {
            let Some(n) = node.child(i as _) else {
                continue;
            };
            if n.kind().eq("term") {
                let term = self.visit_term(context, n)?;
                stmt.add_select_term(term);
            }
        }

        Ok(())
    }

    pub(crate) fn visit_term(&self, context: &ParseContext, node: Node) -> RS<SelectTerm> {
        let mut term = SelectTerm::new();
        let opt_expression = node.child_by_field_name(ts_field_name::EXPRESSION);
        match opt_expression {
            Some(expression) => {
                self.visit_projection_expression(context, expression, &mut term)?;
                let opt_alias_name = node.child_by_field_name(ts_field_name::ALIAS);
                if let Some(alias) = opt_alias_name {
                    let alias = self.visit_alias_name(context, alias)?;
                    term.set_alias(alias);
                }
            }
            None => {
                let opt_all_fields = node.child_by_field_name(ts_field_name::ALL_FIELDS);
                match opt_all_fields {
                    Some(_) => {}
                    None => {
                        return Err(mudu_error!(ErrorCode::Parse, "no term found"));
                    }
                };
            }
        };
        Ok(term)
    }

    pub(crate) fn visit_projection_expression(
        &self,
        context: &ParseContext,
        node: Node,
        term: &mut SelectTerm,
    ) -> RS<()> {
        let opt_identifier = node.child_by_field_name(ts_field_name::QUALIFIED_FIELD);
        if let Some(n) = opt_identifier {
            let field = self.visit_qualified_field(context, n)?;
            term.set_field(SelectField::Column(field));
            return Ok(());
        }
        let opt_invocation = node.child_by_field_name(ts_field_name::INVOCATION);
        match opt_invocation {
            Some(n) => {
                let function = self.visit_invocation(context, n)?;
                term.set_field(SelectField::Function(function));
            }
            None => return Err(mudu_error!(ErrorCode::NotImplemented)),
        };
        Ok(())
    }

    /// Parse a function invocation (`name(arg)`) in a select list.
    ///
    /// Only a single parameter is supported: either `*` (all fields) or a
    /// plain column reference. `DISTINCT`, multiple parameters, and in-call
    /// `ORDER BY` are rejected as not implemented.
    pub(crate) fn visit_invocation(&self, context: &ParseContext, node: Node) -> RS<ExprFunction> {
        let mut name = None;
        for i in 0..node.child_count() {
            let Some(child) = node.child(i as _) else {
                continue;
            };
            if child.kind().eq("object_reference") {
                name = Some(self.visit_object_reference(context, child)?);
                break;
            }
        }
        let name = match name {
            Some(name) => name,
            None => {
                return Err(mudu_error!(
                    ErrorCode::Parse,
                    format!(
                        "no function name in {}",
                        ts_node_context_string(context.parse_str(), &node)?
                    )
                ))
            }
        };

        let mut cursor = node.walk();
        let params: Vec<Node> = node
            .children_by_field_name(ts_field_name::PARAMETER, &mut cursor)
            .collect();
        if params.len() != 1 {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("function {} requires exactly one argument", name)
            ));
        }
        // Reject the extended invocation forms (DISTINCT, in-call ORDER BY,
        // FILTER, separator literals, LIMIT) instead of silently dropping
        // them: only `name(arg)` with a single plain parameter is supported.
        for i in 0..node.child_count() {
            let Some(child) = node.child(i as _) else {
                continue;
            };
            let kind = child.kind();
            let allowed =
                kind.eq("object_reference") || kind.eq("(") || kind.eq(")") || kind.eq("term");
            if !allowed {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    format!("unsupported form in function {} invocation: {}", name, kind)
                ));
            }
        }
        let param = params[0];
        let arg = if param
            .child_by_field_name(ts_field_name::ALL_FIELDS)
            .is_some()
        {
            FunctionArg::Star
        } else {
            let opt_expression = param.child_by_field_name(ts_field_name::EXPRESSION);
            let expression = rs_option(opt_expression, "no expression in function argument")?;
            let opt_field = expression.child_by_field_name(ts_field_name::QUALIFIED_FIELD);
            match opt_field {
                Some(n) => FunctionArg::Column(self.visit_qualified_field(context, n)?),
                None => {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        format!(
                            "unsupported argument to function {}: only `*` or a column reference is supported",
                            name
                        )
                    ))
                }
            }
        };
        Ok(ExprFunction::new(name, arg))
    }

    pub(crate) fn visit_alias_name(&self, context: &ParseContext, node: Node) -> RS<String> {
        let opt_alias = node.child_by_field_name(ts_field_name::ALIAS);
        match opt_alias {
            None => Err(mudu_error!(
                ErrorCode::Parse,
                format!(
                    "alias not found in {}",
                    ts_node_context_string(context.parse_str(), &node)?
                )
            )),
            Some(n) => {
                let s = ts_node_context_string(context.parse_str(), &n)?;
                Ok(s)
            }
        }
    }
}
