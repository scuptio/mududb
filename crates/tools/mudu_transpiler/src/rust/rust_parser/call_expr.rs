//! Call-expression visitors: collect callee identifiers and argument spans so
//! the context can record call dependencies and async-conversion positions.

use super::{RustParser, expected_child_filed};
use crate::rust::parse_context::{ParseContext, Position};
use crate::rust::ts_const;
use mudu::common::result::RS;
use tree_sitter::Node;

#[derive(Debug)]
struct CallArguments {
    text: String,
    end_position: Position,
}

#[derive(Debug)]
struct CallIdentifier {
    name: String,
}

#[derive(Debug)]
enum CallExprKind {
    Identifier(String),
    Arguments(CallArguments),
}

impl RustParser {
    pub(super) fn visit_call_expression(
        &self,
        context: &mut ParseContext,
        node: Node,
        opt_caller: &Option<String>,
    ) -> RS<()> {
        let mut call_chains = Vec::new();
        self.visit_call_expression_inner(context, node, &mut call_chains)?;
        if let Some(caller) = opt_caller {
            for (identifier, arguments) in call_chains.iter() {
                context.add_call_dependency(caller, &identifier.name);
                let is_sys_call = context.is_sys_call(&identifier.name);
                context.add_func_call_end_position(
                    identifier.name.clone(),
                    arguments.end_position,
                    is_sys_call,
                );
            }
        }
        Ok(())
    }

    fn visit_function_inner(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<Option<CallIdentifier>> {
        let kind = node.kind();
        match kind {
            ts_const::ts_kind_name::S_IDENTIFIER => {
                let name = context.node_text(&node)?;
                Ok(Some(CallIdentifier { name }))
            }
            ts_const::ts_kind_name::S_GENERIC_FUNCTION => {
                let function = expected_child_filed(&node, ts_const::ts_field_name::FUNCTION)?;
                self.visit_function_inner(context, function)
            }
            _ => {
                for i in 0..node.child_count() {
                    let n = node.child_count() - i;
                    let opt_child = node.child(n as _);
                    if let Some(c) = opt_child {
                        let opt_ident = self.visit_function_inner(context, c)?;
                        if opt_ident.is_some() {
                            return Ok(opt_ident);
                        }
                    }
                }
                Ok(None)
            }
        }
    }
    fn visit_function(&self, context: &ParseContext, node: Node) -> RS<Option<CallIdentifier>> {
        self.visit_function_inner(context, node)
    }

    fn visit_call_expression_inner(
        &self,
        context: &ParseContext,
        node: Node,
        call_chains: &mut Vec<(CallIdentifier, CallArguments)>,
    ) -> RS<()> {
        if node.kind() == ts_const::ts_kind_name::S_CALL_EXPRESSION {
            let function = expected_child_filed(&node, ts_const::ts_field_name::FUNCTION)?;
            let opt_identifier = self.visit_function(context, function)?;
            if let Some(identifier) = opt_identifier {
                let arguments = expected_child_filed(&node, ts_const::ts_field_name::ARGUMENTS)?;
                let call_arguments = self.visit_call_arguments(context, arguments)?;
                call_chains.push((identifier, call_arguments))
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_call_expression_inner(context, child, call_chains)?;
        }
        Ok(())
    }

    fn visit_call_arguments(&self, context: &ParseContext, node: Node) -> RS<CallArguments> {
        let text = context.node_text(&node)?;
        let pos = Position::from_ts(node.end_position());
        Ok(CallArguments {
            text,
            end_position: pos,
        })
    }
    fn visit_sub_check_is_sys_call(&self, context: &mut ParseContext, node: Node) -> RS<bool> {
        if node.kind() != ts_const::ts_kind_name::S_IDENTIFIER {
            let node_text = context.node_text(&node)?;
            if context.is_sys_call(&node_text) {
                return Ok(true);
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let is_sys_call = self.visit_sub_check_is_sys_call(context, child)?;
            if is_sys_call {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
