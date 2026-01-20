use crate::rust::parse_context::{ParseContext, Position, UseRefactor};
use crate::rust::ts_const;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::collections::HashMap;

use crate::rust::function::Function;
use crate::rust::rust_type::RustType;
use tree_sitter::{Language, Node, Parser};

const MUDU_PROC_MARKER: &str = "/**mudu-proc**/";

pub struct RustParser {}

fn rust_language() -> Language {
    tree_sitter_rust::LANGUAGE.into()
}

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
    fn new() -> RustParser {
        Self {}
    }

    pub fn parse(context: &mut ParseContext) -> RS<()> {
        let mut parser = Parser::new();
        parser.set_language(&rust_language()).unwrap();
        let tree = parser.parse(&context.text, None).unwrap();
        let node = tree.root_node();
        let parser = Self::new();
        parser.walk_node(context, node, &None)?;
        Ok(())
    }

    fn walk_node(
        &self,
        context: &mut ParseContext,
        node: Node,
        opt_function_name: &Option<String>,
    ) -> RS<()> {
        let mut cursor = node.walk();
        for (_, child) in node.children(&mut cursor).enumerate() {
            let kind = child.kind();
            match kind {
                ts_const::ts_kind_name::S_USE_DECLARATION => {
                    self.visit_use_declaration(context, child)?;
                }
                ts_const::ts_kind_name::S_FUNCTION_ITEM => {
                    self.visit_function_item(context, child)?;
                }
                ts_const::ts_kind_name::S_CALL_EXPRESSION => {
                    self.visit_call_expression(context, child, opt_function_name)?;
                }
                _ => {
                    self.walk_node(context, child, opt_function_name)?;
                }
            }
        }
        Ok(())
    }

    fn is_mudu_proc(&self, context: &ParseContext, function_item_start_row: usize) -> bool {
        if context.lines.len() < function_item_start_row {
            panic!("row index out of bounds");
        }
        for i in 1..function_item_start_row {
            let line = &context.lines[function_item_start_row - i];
            let line_trim = line.trim();
            if line_trim == MUDU_PROC_MARKER {
                return true;
            } else if !line_trim.is_empty() {
                return false;
            }
        }
        false
    }

    fn visit_use_declaration(&self, context: &mut ParseContext, node: Node) -> RS<()> {
        let (stack, src, dst) = match &context.refactor_src_dst_mod {
            Some((src, dst)) => {
                let mut stack = Vec::new();
                let mut next_identifier = true;
                let mut path = Vec::new();
                self.visit_use_declaration_inner(
                    context,
                    node,
                    &mut next_identifier,
                    &mut path,
                    &mut stack,
                )?;
                if stack.is_empty() {
                    return Ok(());
                } else {
                    (stack, src.clone(), dst.clone())
                }
            }
            None => {
                return Ok(());
            }
        };
        let path_list = self.build_path_list(&stack);
        for path in path_list {
            self.refactor_use_mod(context, &path, &src, &dst)?;
        }
        Ok(())
    }

    fn build_path_list(
        &self,
        stack: &Vec<HashMap<String, (Option<String>, Position, Position)>>,
    ) -> Vec<Vec<(String, Position, Position)>> {
        let opt = stack.last();
        let mut ret = Vec::new();
        if let Some(map) = opt {
            for (name, (opt_parent, start, end)) in map.iter() {
                let mut vec = Vec::new();
                let name = name.clone();
                let start = start.clone();
                let end = end.clone();
                vec.push((name, start, end));
                let mut i = stack.len() - 1;
                let mut opt_parent = opt_parent.clone();
                while i >= 1 {
                    match &opt_parent {
                        Some(parent) => {
                            let parent_level = &stack[i - 1];
                            let opt = parent_level.get(parent);
                            if let Some((opt_p, s, e)) = opt {
                                vec.push((parent.clone(), s.clone(), e.clone()));
                                opt_parent = opt_p.clone()
                            }
                            i -= 1;
                        }
                        _ => {
                            break;
                        }
                    }
                }
                vec.reverse();
                ret.push(vec)
            }
        }
        ret
    }

    fn refactor_use_mod(
        &self,
        context: &mut ParseContext,
        path: &Vec<(String, Position, Position)>,
        src: &Vec<String>,
        dst: &Vec<String>,
    ) -> RS<()> {
        if src.len() != dst.len() {
            panic!(
                "cannot possible src len ({}) != dst len ({})",
                src.len(),
                dst.len()
            );
        }
        let mut i = 0;
        let mut j = 0;
        let mut matches = Vec::new();
        while i < path.len() && j < dst.len() {
            let (path_i_str, start_pos, end_pos) = &path[i];
            let path_j_str = &src[j];
            if path_i_str == path_j_str {
                i += 1;
                j += 1;
                matches.push((path_i_str.clone(), start_pos.clone(), end_pos.clone()));
            } else {
                i += 1;
                j = 0;
                matches.clear()
            }
        }
        if matches.len() == src.len() {
            for (i, (_, start_position, end_position)) in matches.into_iter().enumerate() {
                context.position_refactor_use.push(UseRefactor {
                    start_position,
                    end_position,
                    src_string: src[i].to_string(),
                    dst_string: dst[i].to_string(),
                })
            }
        }
        Ok(())
    }
    fn visit_use_declaration_inner(
        &self,
        context: &ParseContext,
        node: Node,
        find_next_identifier: &mut bool,
        path: &mut Vec<String>,
        stack: &mut Vec<HashMap<String, (Option<String>, Position, Position)>>,
    ) -> RS<()> {
        let mut new_level_parent = path.last().cloned();
        let mut increase_depth = false;
        if node.kind() == ts_const::ts_kind_name::S_IDENTIFIER {
            let identifier = context.node_text(&node)?;
            let start_position = Position::from_ts(node.start_position());
            let end_position = Position::from_ts(node.end_position());
            if *find_next_identifier {
                increase_depth = true;
            }
            if increase_depth {
                let mut map = HashMap::new();
                map.insert(
                    identifier.clone(),
                    (new_level_parent.clone(), start_position, end_position),
                );
                stack.push(map);
                path.push(identifier.clone());
            } else {
                if let Some(last) = stack.last_mut() {
                    last.insert(
                        identifier.clone(),
                        (new_level_parent.clone(), start_position, end_position),
                    );
                }
                path.pop();
                path.push(identifier);
            }
            *find_next_identifier = false;
        } else if node.kind() == "::" {
            *find_next_identifier = true;
        } else if node.kind() == ts_const::ts_kind_name::S_USE_LIST {
            let mut cursor = node.walk();
            let mut vec = Vec::new();
            for child in node.children(&mut cursor) {
                if child.kind() != "," && child.kind() != "{" && child.kind() != "}" {
                    vec.push(child);
                }
            }
            for child in vec {
                self.visit_use_declaration_inner(
                    context,
                    child,
                    find_next_identifier,
                    path,
                    stack,
                )?;
            }
        } else {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.visit_use_declaration_inner(
                    context,
                    child,
                    find_next_identifier,
                    path,
                    stack,
                )?;
            }
        }

        Ok(())
    }

    fn visit_function_item(&self, context: &mut ParseContext, node: Node) -> RS<()> {
        let mut cursor = node.walk();
        let mut contains_async = false;
        let mut fn_start_pos = None;
        let start_pos = node.start_position();
        let is_mudu_proc = self.is_mudu_proc(context, start_pos.row);
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
                return Err(m_error!(
                    EC::NoneErr,
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
                let rust_type = self.visit_type(&context, child)?;
                vec.push(rust_type);
            }
        }
        Ok(vec)
    }

    fn visit_return_type(&self, context: &ParseContext, node: Node) -> RS<RustType> {
        self.visit_type(context, node)
    }

    fn visit_call_expression(
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
                    arguments.end_position.clone(),
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
        match node.kind() {
            ts_const::ts_kind_name::S_CALL_EXPRESSION => {
                let function = expected_child_filed(&node, ts_const::ts_field_name::FUNCTION)?;
                let opt_identifier = self.visit_function(context, function)?;
                if let Some(identifier) = opt_identifier {
                    let arguments =
                        expected_child_filed(&node, ts_const::ts_field_name::ARGUMENTS)?;
                    let call_arguments = self.visit_call_arguments(context, arguments)?;
                    call_chains.push((identifier, call_arguments))
                }
            }
            _ => {}
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

fn expected_child_filed<'tree>(node: &Node<'tree>, field: &str) -> RS<Node<'tree>> {
    let child = node.child_by_field_name(field).map_or_else(
        || {
            Err(m_error!(
                EC::NoneErr,
                format!("cannot find child filed for {}", field)
            ))
        },
        |child| Ok(child),
    )?;
    Ok(child)
}
fn opt_child_filed<'tree>(node: &Node<'tree>, field: &str) -> Option<Node<'tree>> {
    let child = node.child_by_field_name(field)?;
    Some(child)
}

#[cfg(test)]
mod tests {
    use crate::rust::parse_context::ParseContext;
    use crate::rust::rust_parser::RustParser;

    #[test]
    fn test_rust_parser() {
        let text_rs = include_str!("test_rs/proc1.rs");
        let mut c = ParseContext::new(
            text_rs.to_string(),
            Some("wasm".to_string()),
            Some("wasm_generated".to_string()),
        );
        RustParser::parse(&mut c).unwrap();
        c.tran_to_async();
        let s = c.render_source("app_test".to_string(), true).unwrap();
        println!("{}", s);
    }
}
