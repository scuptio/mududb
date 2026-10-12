//! `use` declaration visitors: collect scoped identifier paths and record the
//! rewrite positions for a `refactor_src_dst_mod` module rename.

use super::RustParser;
use crate::rust::parse_context::{ParseContext, Position, UseRefactor};
use crate::rust::ts_const;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::collections::HashMap;
use tree_sitter::Node;

type ScopeMap = HashMap<String, (Option<String>, Position, Position)>;

impl RustParser {
    pub(super) fn visit_use_declaration(&self, context: &mut ParseContext, node: Node) -> RS<()> {
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

    fn build_path_list(&self, stack: &[ScopeMap]) -> Vec<Vec<(String, Position, Position)>> {
        let opt = stack.last();
        let mut ret = Vec::new();
        if let Some(map) = opt {
            for (name, (opt_parent, start, end)) in map.iter() {
                let mut vec = Vec::new();
                let name = name.clone();
                vec.push((name, *start, *end));
                let mut i = stack.len() - 1;
                let mut opt_parent = opt_parent.clone();
                while i >= 1 {
                    match &opt_parent {
                        Some(parent) => {
                            let parent_level = &stack[i - 1];
                            let opt = parent_level.get(parent);
                            if let Some((opt_p, s, e)) = opt {
                                vec.push((parent.clone(), *s, *e));
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
        path: &[(String, Position, Position)],
        src: &[String],
        dst: &[String],
    ) -> RS<()> {
        if src.len() != dst.len() {
            return Err(mudu_error!(
                ErrorCode::Internal,
                format!(
                    "cannot possible src len ({}) != dst len ({})",
                    src.len(),
                    dst.len()
                )
            ));
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
                matches.push((path_i_str.clone(), *start_pos, *end_pos));
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
        stack: &mut Vec<ScopeMap>,
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
}
