//! Field metadata extracted from a column definition.

use crate::lang_impl::lang::lang_data_type::uni_data_type_to_name;
use crate::lang_impl::lang::lang_kind::LangKind;
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_snake_case, to_snake_case_upper};
use mudu_binding::table::column_def::ColumnDef;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_data_value::UniDataValue;
use mudu_binding::universal::uni_scalar::UniScalar;
use mudu_binding::universal::uni_scalar_value::UniScalarValue;

/// Metadata for a single field/column of a generated entity.
#[derive(Debug)]
pub struct FieldInfo {
    /// Raw column name.
    pub field_name: String,
    /// Snake-case field name.
    pub field_name_snake_case: String,
    /// Upper-snake-case constant name for the field.
    pub field_name_const: String,
    /// Language-specific base type name (e.g. `i32`, `String`).
    pub data_type: String,
    /// Rust field type: the base type for `NOT NULL` columns,
    /// `Option<base>` for nullable columns.
    pub rust_field_type: String,
    /// Rust expression evaluating to the column [`DataType`] used in the
    /// tuple description; carries DDL type parameters (e.g. `NUMERIC(p, s)`
    /// precision/scale) when present, otherwise the type-family default.
    pub datum_desc_type_expr: String,
    /// `true` if the column is `NOT NULL` (primary key columns imply it).
    pub is_not_null: bool,
    /// `true` if the column accepts SQL `NULL` (the inverse of `is_not_null`).
    pub is_nullable: bool,
    /// `true` if the column is part of the primary key.
    pub is_primary_key: bool,
    /// `true` if the Rust field type is `Copy` (scalar numerics, bool, char).
    pub is_copy: bool,
}

impl FieldInfo {
    /// Build [`FieldInfo`] from a [`ColumnDef`] and target language.
    pub fn from_column_schema(
        _table_name: &str,
        column_schema: &ColumnDef,
        lang: &LangKind,
    ) -> RS<Self> {
        let data_type = uni_data_type_to_name(column_schema.data_type(), lang)?;
        let is_not_null = column_schema.is_not_null();
        let rust_field_type = if is_not_null {
            data_type.clone()
        } else {
            format!("Option<{data_type}>")
        };
        let datum_desc_type_expr = match datum_desc_type_override(column_schema, lang) {
            Some(override_expr) => override_expr,
            None => format!("<{data_type} as Datum>::data_type()"),
        };
        Ok(Self {
            field_name: column_schema.column_name().clone(),
            field_name_snake_case: to_snake_case(column_schema.column_name()),
            field_name_const: to_snake_case_upper(column_schema.column_name()),
            data_type,
            rust_field_type,
            datum_desc_type_expr,
            is_not_null,
            is_nullable: !is_not_null,
            is_primary_key: column_schema.is_primary_key(),
            is_copy: is_copy_type(column_schema.data_type()),
        })
    }
}

/// `true` if the Rust mapping of the column type is a `Copy` scalar.
fn is_copy_type(data_type: &UniDataType) -> bool {
    matches!(
        data_type,
        UniDataType::Scalar(
            UniScalar::Bool
                | UniScalar::U8
                | UniScalar::U16
                | UniScalar::U32
                | UniScalar::U64
                | UniScalar::U128
                | UniScalar::I8
                | UniScalar::I16
                | UniScalar::I32
                | UniScalar::I64
                | UniScalar::I128
                | UniScalar::F32
                | UniScalar::F64
                | UniScalar::Char
        )
    )
}

/// Build a tuple-description [`DataType`] expression that preserves DDL type
/// parameters, so the generated entity encodes values with the schema's
/// precision/scale instead of the type-family default. Currently only
/// `NUMERIC(p, s)` for Rust.
fn datum_desc_type_override(column_schema: &ColumnDef, lang: &LangKind) -> Option<String> {
    if !matches!(lang, LangKind::Rust) {
        return None;
    }
    if !matches!(
        column_schema.data_type(),
        UniDataType::Scalar(UniScalar::Numeric)
    ) {
        return None;
    }
    let params = column_schema.data_type_param().as_ref()?;
    if params.len() != 2 {
        return None;
    }
    let mut ints = params.iter().map(|p| match p {
        UniDataValue::Scalar(UniScalarValue::I64(v)) => Some(*v),
        _ => None,
    });
    let precision = ints.next().flatten()?;
    let scale = ints.next().flatten()?;
    Some(format!(
        "DataType::from_id_param(\n            TypeFamily::Numeric,\n            \
         Some(mududb::types::data_type_param_kind::DataTypeParamKind::Numeric(Box::new(\n                \
         mududb::types::data_type_param_numeric::DataTypeParamNumeric::new({precision}, {scale}),\n            \
         ))),\n        )"
    ))
}
