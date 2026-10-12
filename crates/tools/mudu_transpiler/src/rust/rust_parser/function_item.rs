//! Function-item visitors: register `/**mudu-proc**/` functions and decode
//! their parameter and return types into `RustType`s.

use super::{RustParser, expected_child_filed, opt_child_filed};
use crate::rust::function::Function;
use crate::rust::parse_context::{ParseContext, Position};
use crate::rust::rust_type::RustType;
use crate::rust::ts_const;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tree_sitter::Node;

impl RustParser {
    pub(super) fn visit_function_item(&self, context: &mut ParseContext, node: Node) -> RS<()> {
        let mut cursor = node.walk();
        let mut contains_async = false;
        let mut fn_start_pos = None;
        let start_pos = node.start_position();
        let is_mudu_proc = self.is_mudu_proc(context, start_pos.row)?;
        for child in node.children(&mut cursor) {
            match child.kind() {
                "fn" => {
                    let pos = child.start_position();
                    fn_start_pos = Some(Position::from_ts(pos));
                }
                ts_const::ts_kind_name::S_FUNCTION_MODIFIERS => {
                    let text = context.node_text(&child)?;
                    contains_async = text.contains("async");
                }
                _ => {}
            }
        }

        let name_node = expected_child_filed(&node, ts_const::ts_field_name::NAME)?;
        let body_node = expected_child_filed(&node, ts_const::ts_field_name::BODY)?;
        let function_name = context.node_text(&name_node)?;
        if let Some(pos) = fn_start_pos {
            context
                .position_fn_start
                .insert(function_name.clone(), (pos, contains_async));
        }
        if is_mudu_proc {
            let parameters = expected_child_filed(&node, ts_const::ts_field_name::PARAMETERS)?;
            let vec_parameters = self.visit_parameters(context, parameters)?;
            let opt_return_type = opt_child_filed(&node, ts_const::ts_field_name::RETURN_TYPE);
            let opt_ret_rust_type = match opt_return_type {
                Some(return_type) => Some(self.visit_type(context, return_type)?),
                _ => None,
            };
            let function = Function {
                name: function_name.clone(),
                arg_list: vec_parameters,
                return_type: opt_ret_rust_type,
                is_async: false,
            };
            context
                .mudu_procedure
                .insert(function.name.clone(), function);
        }
        self.walk_node(context, body_node, &Some(function_name))?;
        Ok(())
    }

    fn visit_parameters(
        &self,
        parse_context: &ParseContext,
        node: Node,
    ) -> RS<Vec<(String, RustType)>> {
        let mut cursor = node.walk();
        let mut vec = Vec::new();
        for child in node.children(&mut cursor) {
            if child.kind() == ts_const::ts_kind_name::S_PARAMETER {
                let (name, ty) = self.visit_parameter(parse_context, child)?;
                vec.push((name, ty));
            }
        }
        Ok(vec)
    }

    fn visit_parameter(&self, context: &ParseContext, node: Node) -> RS<(String, RustType)> {
        let pattern_node = expected_child_filed(&node, ts_const::ts_field_name::PATTERN)?;
        let type_node = expected_child_filed(&node, ts_const::ts_field_name::TYPE)?;
        let rust_type = self.visit_type(context, type_node)?;
        let argument_name = context.node_text(&pattern_node)?;
        Ok((argument_name, rust_type))
    }

    fn visit_type(&self, context: &ParseContext, node: Node) -> RS<RustType> {
        let type_name = context.node_text(&node)?;
        let rust_type = match node.kind() {
            ts_const::ts_kind_name::S_PRIMITIVE_TYPE => RustType::Primitive(type_name),
            ts_const::ts_kind_name::S_TYPE_IDENTIFIER => RustType::Custom(type_name),
            ts_const::ts_kind_name::S_TUPLE_TYPE => {
                let vec = self.visit_tuple_type(context, node)?;
                RustType::Tuple(vec)
            }
            ts_const::ts_kind_name::S_GENERIC_TYPE => {
                let (s, vec) = self.visit_generic_type(context, node)?;
                RustType::Generic(s, vec)
            }
            ts_const::ts_kind_name::S_UNIT_TYPE => RustType::Tuple(Vec::new()),
            _ => {
                let kind = node.kind();
                let text = context.node_text(&node)?;
                return Err(mudu_error!(
                    ErrorCode::UnsupportedOperation,
                    format!("node kind {}, do not support type {}", kind, text)
                ));
            }
        };
        Ok(rust_type)
    }

    fn visit_generic_type(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<(String, Vec<RustType>)> {
        let field = expected_child_filed(&node, ts_const::ts_field_name::TYPE)?;
        let type_arguments = expected_child_filed(&node, ts_const::ts_field_name::TYPE_ARGUMENTS)?;
        let s = context.node_text(&field)?;
        let vec = self.visit_generic_type_arguments(context, type_arguments)?;
        Ok((s, vec))
    }

    fn visit_generic_type_arguments(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<Vec<RustType>> {
        let mut cursor = node.walk();
        let mut vec = Vec::new();
        for (i, c) in node.children(&mut cursor).enumerate() {
            let kind = c.kind();
            if i != 0
                && i + 1 != node.child_count()
                && kind != ","
                && kind != ts_const::ts_kind_name::S_BLOCK
                && kind != ts_const::ts_kind_name::S_LIFETIME
                && kind != ts_const::ts_kind_name::S_TYPE_BINDING
                && !kind.contains(ts_const::ts_kind_name::S__LITERAL)
            {
                let ty = self.visit_type(context, c)?;
                vec.push(ty);
            }
        }
        Ok(vec)
    }
    fn visit_tuple_type(&self, context: &ParseContext, node: Node) -> RS<Vec<RustType>> {
        let mut cursor = node.walk();
        let mut vec = Vec::new();
        for child in node.children(&mut cursor) {
            let k = child.kind();
            if k != "(" && k != "," && k != ")" {
                let rust_type = self.visit_type(context, child)?;
                vec.push(rust_type);
            }
        }
        Ok(vec)
    }

    fn visit_return_type(&self, context: &ParseContext, node: Node) -> RS<RustType> {
        self.visit_type(context, node)
    }
}
