//! WIT enum- and variant-item parsing methods of [`WitParser`].

use super::{ParseContext, WitParser, expected_get_filed};
use crate::ts_const;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_def::{EnumCase, UniEnumDef, UniVariantDef, VariantCase};
use tree_sitter::Node;

impl WitParser {
    pub(super) fn visit_enum_item(&self, context: &ParseContext, node: &Node) -> RS<UniEnumDef> {
        let mut enum_comments = String::new();
        if let Some(comment) = node.child_by_field_name(ts_const::ts_field_name::COMMENT) {
            enum_comments = comment
                .utf8_text(context.text().as_bytes())
                .map_err(|e| mudu_error!(ErrorCode::Parse, "parse error", e))?
                .to_string();
        }
        let mut enum_def = UniEnumDef {
            enum_comments,
            enum_name: String::new(),
            enum_cases: Vec::new(),
        };

        // get enum name
        if let Some(name_node) = node.child_by_field_name(ts_const::ts_field_name::ENUM_NAME) {
            enum_def.enum_name = context.text_of_node(&name_node)?;
        }

        let mut case_cursor = node.walk();
        for (i, case_node) in node
            .children_by_field_name(ts_const::ts_field_name::ENUM_CASE, &mut case_cursor)
            .enumerate()
        {
            let mut comments = String::new();
            if let Some(comment) = case_node.child_by_field_name(ts_const::ts_field_name::COMMENT) {
                comments = comment
                    .utf8_text(context.text().as_bytes())
                    .map_err(|e| {
                        mudu_error!(ErrorCode::Parse, "parse error to retrieve comments", e)
                    })?
                    .to_string();
            }

            let node_case_name =
                expected_get_filed(&case_node, ts_const::ts_field_name::ENUM_CASE_NAME)?;
            let case_name = context.text_of_node(&node_case_name)?;
            let enum_case = EnumCase {
                ec_name: case_name,
                ec_comments: comments,
                ec_number: i as _,
            };
            enum_def.enum_cases.push(enum_case);
        }
        if enum_def.enum_cases.is_empty() {
            return Err(mudu_error!(ErrorCode::Parse, "enum_cases is empty"));
        }
        Ok(enum_def)
    }

    pub(super) fn visit_variant_item(
        &self,
        context: &ParseContext,
        node: &Node,
    ) -> RS<UniVariantDef> {
        let mut variant_def = UniVariantDef {
            variant_comments: String::new(),
            variant_name: String::new(),
            variant_cases: Vec::new(),
        };

        if let Some(comment) = node.child_by_field_name(ts_const::ts_field_name::COMMENT) {
            let comments = comment
                .utf8_text(context.text().as_bytes())
                .map_err(|e| mudu_error!(ErrorCode::Parse, "parse error", e))?
                .to_string();
            variant_def.variant_comments = comments;
        }
        let name_node = expected_get_filed(node, ts_const::ts_field_name::VARIANT_NAME)?;
        variant_def.variant_name = context.text_of_node(&name_node)?;

        let mut case_cursor = node.walk();
        for case_node in
            node.children_by_field_name(ts_const::ts_field_name::VARIANT_CASE, &mut case_cursor)
        {
            let case = self.visit_variant_case(context, &case_node)?;
            variant_def.variant_cases.push(case);
        }
        Ok(variant_def)
    }

    fn visit_variant_case(&self, context: &ParseContext, node: &Node) -> RS<VariantCase> {
        let mut variant_case_def = VariantCase {
            vc_comments: String::new(),
            vc_case_name: String::new(),
            vc_case_type: None,
        };

        if let Some(comment) = node.child_by_field_name(ts_const::ts_field_name::COMMENT) {
            let comments = comment
                .utf8_text(context.text().as_bytes())
                .map_err(|e| mudu_error!(ErrorCode::Parse, "parse error", e))?
                .to_string();
            variant_case_def.vc_comments = comments;
        }

        let name_node = expected_get_filed(node, ts_const::ts_field_name::VARIANT_CASE_NAME)?;
        variant_case_def.vc_case_name = context.text_of_node(&name_node)?;

        if let Some(case_type) =
            node.child_by_field_name(ts_const::ts_field_name::VARIANT_CASE_TYPE)
        {
            let inner_type = self.visit_type(context, &case_type)?;
            variant_case_def.vc_case_type = Some(inner_type);
        }
        Ok(variant_case_def)
    }
}
