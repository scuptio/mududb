//! Field metadata extracted from a column definition.

use crate::lang_impl::lang::lang_data_type::uni_data_type_to_name;
use crate::lang_impl::lang::lang_kind::LangKind;
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case, to_snake_case_upper};
use mudu_binding::record::field_def::FieldDef;
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
    /// Pascal-case field name.
    pub field_name_pascal_case: String,
    /// Language-specific type name (e.g. `i32`, `String`).
    pub data_type: String,
    /// Name of the attribute struct the field belongs to.
    pub attr_struct_name: String,
    /// Upper-snake-case constant name for the field.
    pub field_name_const: String,
    /// Optional `attr_data_type()` override body carrying DDL type parameters
    /// (e.g. `NUMERIC(12,2)` precision/scale). Rust only.
    pub attr_data_type_override: Option<String>,
}

impl FieldInfo {
    /// Build [`FieldInfo`] from a [`FieldDef`] and target language.
    pub fn from_column_schema(
        table_name: &str,
        column_schema: &FieldDef,
        lang: &LangKind,
    ) -> RS<Self> {
        Ok(Self {
            field_name: column_schema.column_name().clone(),
            field_name_snake_case: to_snake_case(column_schema.column_name()),
            field_name_pascal_case: to_pascal_case(column_schema.column_name()),
            data_type: uni_data_type_to_name(column_schema.data_type(), lang)?,
            attr_struct_name: to_pascal_case(table_name),
            field_name_const: to_snake_case_upper(column_schema.column_name()),
            attr_data_type_override: attr_data_type_override(column_schema, lang),
        })
    }
}

/// Build an `attr_data_type()` body that preserves DDL type parameters, so the
/// generated entity encodes values with the schema's precision/scale instead of
/// the type-family default. Currently only `NUMERIC(p, s)` for Rust.
fn attr_data_type_override(column_schema: &FieldDef, lang: &LangKind) -> Option<String> {
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
