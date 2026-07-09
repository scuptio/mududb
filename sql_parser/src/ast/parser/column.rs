use super::context::ParseContext;
use super::error::{node_or_descendant_has_kind, ts_node_context_string};
use super::SQLParser;
use crate::ast::column_def::ColumnDef;
use crate::ast::stmt_create_table::StmtCreateTable;
use crate::ts_const::{ts_field_name, ts_kind_id, ts_kind_name};
use mudu::common::id::AttrIndex;
use mudu::common::result::RS;
use mudu::common::result_of::rs_option;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_data_value::UniDataValue;
use mudu_binding::universal::uni_scalar::UniScalar;
use mudu_binding::universal::uni_scalar_value::UniScalarValue;
use std::collections::HashMap;
use std::str::FromStr;
use tree_sitter::Node;

impl SQLParser {
    pub(crate) fn visit_column_definition(
        &self,
        context: &ParseContext,
        node: Node,
        stmt: &mut StmtCreateTable,
    ) -> RS<()> {
        let opt_n = node.child_by_field_name(ts_field_name::COLUMN_NAME);
        let n_column_name = rs_option(opt_n, "")?;
        let column_name = self.visit_identifier(context, n_column_name)?;

        let opt_n = node.child_by_field_name(ts_field_name::DATA_TYPE);
        let n_data_type = rs_option(opt_n, "")?;
        let (data_type, opt_type_params) = self.visit_data_type(context, n_data_type)?;
        let mut column_def = ColumnDef::new(column_name, data_type, opt_type_params);
        let mut cursor = node.walk();
        let iter = node.children_by_field_name(ts_field_name::COLUMN_CONSTRAINT, &mut cursor);
        let mut index_map = HashMap::new();
        for n in iter {
            self.visit_column_constraint(n, &mut column_def, &mut index_map)?;
        }

        stmt.add_column_def(column_def);

        Ok(())
    }

    pub(crate) fn visit_column_constraint(
        &self,
        node: Node,
        column_def: &mut ColumnDef,
        index_map: &mut HashMap<String, AttrIndex>,
    ) -> RS<()> {
        if node
            .child_by_field_name(ts_field_name::PRIMARY_KEY)
            .is_some()
        {
            let next_index = index_map
                .entry(ts_field_name::PRIMARY_KEY.to_string())
                .or_insert(0);
            column_def.set_primary_key_index(Some(*next_index));
            *next_index += 1;
        }
        if node_or_descendant_has_kind(node, ts_kind_name::S__NOT_NULL)
            || (node_or_descendant_has_kind(node, ts_kind_name::S_KEYWORD_NOT)
                && node_or_descendant_has_kind(node, ts_kind_name::S_KEYWORD_NULL))
        {
            column_def.set_nullable(false);
        }
        Ok(())
    }

    pub(crate) fn visit_data_type(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<(UniDataType, Option<Vec<UniDataValue>>)> {
        let opt = node.child_by_field_name(ts_field_name::DATA_TYPE_KIND);
        let n = rs_option(opt, "")?;
        self.visit_data_type_kind(context, n)
    }

    pub(crate) fn visit_data_type_kind(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<(UniDataType, Option<Vec<UniDataValue>>)> {
        let opt_n = node.child(0);
        let child = rs_option(opt_n, "no child in data type kind")?;
        let kind = child.kind_id();
        let ret = match kind {
            ts_kind_id::INT => (UniDataType::Scalar(UniScalar::I32), None),
            ts_kind_id::BIGINT => (UniDataType::Scalar(UniScalar::I64), None),
            ts_kind_id::HUGEINT => (UniDataType::Scalar(UniScalar::I128), None),
            ts_kind_id::DOUBLE => (UniDataType::Scalar(UniScalar::F64), None),
            ts_kind_id::FLOAT => (UniDataType::Scalar(UniScalar::F32), None),
            ts_kind_id::CHAR | ts_kind_id::VARCHAR | ts_kind_id::KEYWORD_TEXT => {
                let opt_params = if kind == ts_kind_id::CHAR || kind == ts_kind_id::VARCHAR {
                    let param = self.visit_char_param(context, child)?;
                    Some(vec![param])
                } else {
                    None
                };
                (UniDataType::Scalar(UniScalar::String), opt_params)
            }
            ts_kind_id::NUMERIC | ts_kind_id::DECIMAL => (
                UniDataType::Scalar(UniScalar::Numeric),
                self.visit_numeric_params(context, child)?,
            ),
            ts_kind_id::KEYWORD_DATE => (UniDataType::Scalar(UniScalar::Date), None),
            ts_kind_id::TIME => (
                UniDataType::Scalar(UniScalar::Time),
                self.visit_optional_precision_param(context, child)?,
            ),
            ts_kind_id::KEYWORD_TIME => (
                UniDataType::Scalar(UniScalar::Time),
                self.visit_optional_precision_param(context, node)?,
            ),
            ts_kind_id::TIMESTAMP => (
                UniDataType::Scalar(UniScalar::Timestamp),
                self.visit_optional_precision_param(context, child)?,
            ),
            ts_kind_id::KEYWORD_TIMESTAMP_BASE => (
                UniDataType::Scalar(UniScalar::Timestamp),
                self.visit_optional_precision_param(context, node)?,
            ),
            ts_kind_id::TIMESTAMPTZ => (
                UniDataType::Scalar(UniScalar::TimestampTz),
                self.visit_optional_precision_param(context, child)?,
            ),
            ts_kind_id::KEYWORD_TIMESTAMPTZ_BASE => (
                UniDataType::Scalar(UniScalar::TimestampTz),
                self.visit_optional_precision_param(context, node)?,
            ),
            _ => {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    format!("Data type {} not yet implemented", child.kind())
                ));
            }
        };

        Ok(ret)
    }

    pub(crate) fn visit_char_param(&self, context: &ParseContext, node: Node) -> RS<UniDataValue> {
        if let Some(n) = node.child_by_field_name(ts_field_name::LENGTH) {
            let s = ts_node_context_string(context.parse_str(), &n)?;
            let r = i64::from_str(s.as_str());
            match r {
                Ok(l) => Ok(UniDataValue::Scalar(UniScalarValue::I64(l))),
                Err(e) => Err(mudu_error!(ErrorCode::Parse, "parse u32 error", e)),
            }
        } else {
            Err(mudu_error!(ErrorCode::Parse, "No child parameter found"))
        }
    }

    pub(crate) fn visit_numeric_params(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<Option<Vec<UniDataValue>>> {
        let mut params = Vec::new();
        if let Some(n) = node.child_by_field_name(ts_field_name::PRECISION) {
            let s = ts_node_context_string(context.parse_str(), &n)?;
            let value = i64::from_str(s.as_str())
                .map_err(|e| mudu_error!(ErrorCode::Parse, "parse precision error", e))?;
            params.push(UniDataValue::Scalar(UniScalarValue::I64(value)));
        }
        if let Some(n) = node.child_by_field_name(ts_field_name::SCALE) {
            let s = ts_node_context_string(context.parse_str(), &n)?;
            let value = i64::from_str(s.as_str())
                .map_err(|e| mudu_error!(ErrorCode::Parse, "parse scale error", e))?;
            params.push(UniDataValue::Scalar(UniScalarValue::I64(value)));
        }
        if params.is_empty() {
            Ok(None)
        } else {
            Ok(Some(params))
        }
    }

    pub(crate) fn visit_optional_precision_param(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<Option<Vec<UniDataValue>>> {
        let opt = node
            .child_by_field_name(ts_field_name::PRECISION)
            .or_else(|| node.child_by_field_name(ts_field_name::SIZE));
        if let Some(n) = opt {
            let s = ts_node_context_string(context.parse_str(), &n)?;
            let value = i64::from_str(s.as_str())
                .map_err(|e| mudu_error!(ErrorCode::Parse, "parse precision error", e))?;
            Ok(Some(vec![UniDataValue::Scalar(UniScalarValue::I64(value))]))
        } else {
            Ok(None)
        }
    }
}
