//! WIT table-item parsing methods of [`WitParser`].

use super::{ParseContext, WitParser, expected_get_filed};
use crate::ts_const;
use mudu::common::result::RS;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_def::{RecordField, UniTableDef};
use tree_sitter::Node;

impl WitParser {
    pub(super) fn visit_table_item(&self, context: &ParseContext, node: &Node) -> RS<UniTableDef> {
        let mut table = UniTableDef {
            table_comments: String::new(),
            table_name: String::new(),
            table_key: vec![],
            table_value: vec![],
        };
        if let Some(comment) = node.child_by_field_name(ts_const::ts_field_name::COMMENT) {
            table.table_comments = context.text_of_node(&comment)?
        }

        // table name
        let name_node = expected_get_filed(node, ts_const::ts_field_name::TABLE_NAME)?;
        table.table_name = context.text_of_node(&name_node)?;

        let key_node = expected_get_filed(node, ts_const::ts_field_name::TABLE_KEY)?;
        table.table_key = self.visit_table_key_or_value(context, &key_node)?;
        let value_node = expected_get_filed(node, ts_const::ts_field_name::TABLE_VALUE)?;
        table.table_value = self.visit_table_key_or_value(context, &value_node)?;
        Ok(table)
    }

    fn visit_table_key_or_value(
        &self,
        context: &ParseContext,
        node: &Node,
    ) -> RS<Vec<RecordField>> {
        let _name_ts = context.text_of_node(node)?;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let kind = child.kind();
            match kind {
                ts_const::ts_kind_name::S_ID => {
                    let name = context.text_of_node(&child)?;
                    return Ok(vec![RecordField::new(
                        String::new(),
                        String::new(),
                        UniDataType::Identifier(name),
                    )]);
                }
                ts_const::ts_kind_name::S_RECORD_BODY => {
                    return self.visit_record_body(context, &child);
                }
                _ => {}
            }
        }

        Ok(vec![])
    }
}
