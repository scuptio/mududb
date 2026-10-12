//! WIT function-signature parsing methods of [`WitParser`].

use super::{ParseContext, WitParser, expected_get_filed};
use crate::src_gen::wit_def::WitFuncDef;
use crate::ts_const;
use mudu::common::result::RS;
use mudu_binding::universal::uni_def::RecordField;
use tree_sitter::Node;

impl WitParser {
    pub(super) fn visit_func_item(&self, context: &ParseContext, node: &Node) -> RS<WitFuncDef> {
        let mut func_def = WitFuncDef {
            func_comments: String::new(),
            func_name: String::new(),
            params: Vec::new(),
            returns: Vec::new(),
            func_index: 0,
        };
        if let Some(comment) = node.child_by_field_name(ts_const::ts_field_name::COMMENT) {
            func_def.func_comments = context.text_of_node(&comment)?;
        }
        let name_node = expected_get_filed(node, ts_const::ts_field_name::NAME)?;
        func_def.func_name = context.text_of_node(&name_node)?;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == ts_const::ts_kind_name::S_FUNC_TYPE {
                let (params, returns) = self.visit_func_type(context, &child)?;
                func_def.params = params;
                func_def.returns = returns;
                break;
            }
        }
        Ok(func_def)
    }

    fn visit_func_type(
        &self,
        context: &ParseContext,
        node: &Node,
    ) -> RS<(Vec<RecordField>, Vec<RecordField>)> {
        let mut params = Vec::new();
        let mut returns = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                ts_const::ts_kind_name::S_PARAM_LIST => {
                    params = self.visit_param_list(context, &child)?;
                }
                ts_const::ts_kind_name::S_RESULT_LIST => {
                    returns = self.visit_result_list(context, &child)?;
                }
                _ => {}
            }
        }
        Ok((params, returns))
    }

    fn visit_param_list(&self, context: &ParseContext, node: &Node) -> RS<Vec<RecordField>> {
        let mut fields = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == ts_const::ts_kind_name::S_NAMED_TYPE {
                fields.push(self.visit_named_type(context, &child)?);
            }
        }
        Ok(fields)
    }

    fn visit_result_list(&self, context: &ParseContext, node: &Node) -> RS<Vec<RecordField>> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                ts_const::ts_kind_name::S_TY => {
                    let ty = self.visit_type(context, &child)?;
                    return Ok(vec![RecordField::new(String::new(), String::new(), ty)]);
                }
                ts_const::ts_kind_name::S__NAMED_TYPE_LIST => {
                    let mut fields = Vec::new();
                    let mut inner_cursor = child.walk();
                    for nt in child.children(&mut inner_cursor) {
                        if nt.kind() == ts_const::ts_kind_name::S_NAMED_TYPE {
                            fields.push(self.visit_named_type(context, &nt)?);
                        }
                    }
                    return Ok(fields);
                }
                _ => {}
            }
        }
        Ok(Vec::new())
    }

    fn visit_named_type(&self, context: &ParseContext, node: &Node) -> RS<RecordField> {
        let name_node = expected_get_filed(node, ts_const::ts_field_name::NAME)?;
        let name = context.text_of_node(&name_node)?;
        let type_node = expected_get_filed(node, ts_const::ts_field_name::TYPE)?;
        let ty = self.visit_type(context, &type_node)?;
        Ok(RecordField::new(String::new(), name, ty))
    }
}
