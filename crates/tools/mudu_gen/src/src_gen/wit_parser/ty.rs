//! WIT type-node parsing methods of [`WitParser`].

use super::{ParseContext, WitParser, expected_get_filed};
use crate::ts_const;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_result_type::UniResultType;
use mudu_binding::universal::uni_scalar::UniScalar;
use tree_sitter::Node;

impl WitParser {
    pub(super) fn visit_type(&self, context: &ParseContext, node: &Node) -> RS<UniDataType> {
        let child = node.child(0).map_or_else(
            || {
                Err(mudu_error!(
                    ErrorCode::Parse,
                    "parse error, expected has a child of type node"
                ))
            },
            Ok,
        )?;
        let ty = self.parse_type_node(context, &child)?;
        Ok(ty)
    }

    fn parse_tuple_type(&self, context: &ParseContext, node: &Node) -> RS<UniDataType> {
        let child = expected_get_filed(node, ts_const::ts_field_name::TUPLE_LIST)?;
        let mut cursor = child.walk();
        let mut vec = Vec::new();
        for n in child.children(&mut cursor) {
            if n.kind() == ts_const::ts_kind_name::S_TY {
                let wit_ty = self.visit_type(context, &n)?;
                vec.push(wit_ty);
            }
        }
        Ok(UniDataType::Tuple(vec))
    }

    fn parse_list_type(&self, context: &ParseContext, node: &Node) -> RS<UniDataType> {
        let ty_node = expected_get_filed(node, ts_const::ts_field_name::LIST_INNER_TYPE)?;
        let ty_inner = self.visit_type(context, &ty_node)?;
        if matches!(
            &ty_inner,
            UniDataType::Scalar(UniScalar::U8) | UniDataType::Scalar(UniScalar::I8)
        ) {
            Ok(UniDataType::Binary)
        } else {
            Ok(UniDataType::Array(Box::new(ty_inner)))
        }
    }

    fn parse_option_type(&self, context: &ParseContext, node: &Node) -> RS<UniDataType> {
        let ty_node = expected_get_filed(node, ts_const::ts_field_name::OPTION_INNER_TYPE)?;
        let ty_inner = self.visit_type(context, &ty_node)?;
        Ok(UniDataType::Option(Box::new(ty_inner)))
    }

    fn parse_result_type(&self, context: &ParseContext, node: &Node) -> RS<UniDataType> {
        let ty_err = node.child_by_field_name(ts_const::ts_field_name::RESULT_ERR_TYPE);
        let opt_err = if let Some(n) = ty_err {
            let ty_err_inner = self.visit_type(context, &n)?;
            Some(Box::new(ty_err_inner))
        } else {
            None
        };

        let ty_ok = node.child_by_field_name(ts_const::ts_field_name::RESULT_OK_TYPE);
        let opt_ok = if let Some(n) = ty_ok {
            let ty_ok_inner = self.visit_type(context, &n)?;
            Some(Box::new(ty_ok_inner))
        } else {
            None
        };

        Ok(UniDataType::Result(UniResultType {
            ok: opt_ok,
            err: opt_err,
        }))
    }

    fn parse_custom_type(&self, context: &ParseContext, node: &Node) -> RS<UniDataType> {
        let type_name = context.text_of_node(node)?;
        if type_name == "blob" {
            return Ok(UniDataType::Scalar(UniScalar::Blob));
        }
        Ok(UniDataType::Identifier(type_name))
    }
    fn parse_box_type(&self, context: &ParseContext, node: &Node) -> RS<UniDataType> {
        let ty_node = expected_get_filed(node, ts_const::ts_field_name::BOX_INNER)?;
        let inner = self.visit_type(context, &ty_node)?;
        Ok(UniDataType::Box(Box::new(inner)))
    }

    fn parse_type_node(&self, context: &ParseContext, node: &Node) -> RS<UniDataType> {
        let ty = match node.kind() {
            ts_const::ts_kind_name::S_BOOL => UniDataType::Scalar(UniScalar::Bool),
            ts_const::ts_kind_name::S_U8 => UniDataType::Scalar(UniScalar::U8),
            ts_const::ts_kind_name::S_U16 => UniDataType::Scalar(UniScalar::U16),
            ts_const::ts_kind_name::S_U32 => UniDataType::Scalar(UniScalar::U32),
            ts_const::ts_kind_name::S_U64 => UniDataType::Scalar(UniScalar::U64),
            ts_const::ts_kind_name::S_S8 => UniDataType::Scalar(UniScalar::I8),
            ts_const::ts_kind_name::S_S16 => UniDataType::Scalar(UniScalar::I16),
            ts_const::ts_kind_name::S_S32 => UniDataType::Scalar(UniScalar::I32),
            ts_const::ts_kind_name::S_S64 => UniDataType::Scalar(UniScalar::I64),
            ts_const::ts_kind_name::S_F32 => UniDataType::Scalar(UniScalar::F32),
            ts_const::ts_kind_name::S_F64 => UniDataType::Scalar(UniScalar::F64),
            ts_const::ts_kind_name::S_CHAR => UniDataType::Scalar(UniScalar::Char),
            ts_const::ts_kind_name::S_STRING => UniDataType::Scalar(UniScalar::String),
            ts_const::ts_kind_name::S_TUPLE => self.parse_tuple_type(context, node)?,
            ts_const::ts_kind_name::S_LIST => self.parse_list_type(context, node)?,
            ts_const::ts_kind_name::S_OPTION => self.parse_option_type(context, node)?,
            ts_const::ts_kind_name::S_RESULT => self.parse_result_type(context, node)?,
            ts_const::ts_kind_name::S_ID => self.parse_custom_type(context, node)?,
            ts_const::ts_kind_name::S_BOX => self.parse_box_type(context, node)?,
            _ => {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    format!(
                        "do not support type kind {}, text {}",
                        node.kind(),
                        context.text_of_node(node)?
                    )
                ));
            }
        };
        Ok(ty)
    }
}
