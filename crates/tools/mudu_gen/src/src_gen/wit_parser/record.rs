//! WIT record-item parsing methods of [`WitParser`].

use super::{ParseContext, WitParser, expected_get_filed};
use crate::ts_const;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_def::{RecordField, UniRecordDef};
use tree_sitter::Node;

impl WitParser {
    pub(super) fn visit_record_item(
        &self,
        context: &ParseContext,
        node: &Node,
    ) -> RS<UniRecordDef> {
        let mut record = UniRecordDef {
            record_comments: String::new(),
            record_name: String::new(),
            record_fields: Vec::new(),
        };
        if let Some(comment) = node.child_by_field_name(ts_const::ts_field_name::COMMENT) {
            record.record_comments = comment
                .utf8_text(context.text().as_bytes())
                .map_err(|e| mudu_error!(ErrorCode::Parse, "parse error", e))?
                .to_string();
        }

        // record name
        let name_node = expected_get_filed(node, ts_const::ts_field_name::RECORD_NAME)?;
        record.record_name = name_node
            .utf8_text(context.text().as_bytes())
            .map_err(|e| mudu_error!(ErrorCode::Parse, "parse error", e))?
            .to_string();

        // fields
        let record_body = expected_get_filed(node, ts_const::ts_field_name::RECORD_BODY)?;
        record.record_fields = self.visit_record_body(context, &record_body)?;
        Ok(record)
    }

    pub(super) fn visit_record_body(
        &self,
        context: &ParseContext,
        node: &Node,
    ) -> RS<Vec<RecordField>> {
        let mut fields = Vec::new();
        let mut field_cursor = node.walk();
        for field_node in
            node.children_by_field_name(ts_const::ts_field_name::RECORD_FIELD, &mut field_cursor)
        {
            let field = self.visit_record_field(context, &field_node)?;
            fields.push(field);
        }
        Ok(fields)
    }

    fn visit_record_field(&self, context: &ParseContext, node: &Node) -> RS<RecordField> {
        let mut field_comments = String::new();
        if let Some(comment) = node.child_by_field_name(ts_const::ts_field_name::COMMENT) {
            field_comments = comment
                .utf8_text(context.text().as_bytes())
                .map_err(|e| mudu_error!(ErrorCode::Parse, "parse error", e))?
                .to_string();
        }
        let n_name = node
            .child_by_field_name(ts_const::ts_field_name::RECORD_FIELD_NAME)
            .map_or_else(
                || {
                    Err(mudu_error!(
                        ErrorCode::Parse,
                        "parse error, expected record field name"
                    ))
                },
                Ok,
            )?;
        let field_name = context.text_of_node(&n_name)?;
        let n_type = node
            .child_by_field_name(ts_const::ts_field_name::RECORD_FIELD_TYPE)
            .map_or_else(
                || {
                    Err(mudu_error!(
                        ErrorCode::Parse,
                        "parse error, expected record field type"
                    ))
                },
                Ok,
            )?;
        let field_type = self.visit_type(context, &n_type)?;
        Ok(RecordField::new(field_comments, field_name, field_type))
    }
}
