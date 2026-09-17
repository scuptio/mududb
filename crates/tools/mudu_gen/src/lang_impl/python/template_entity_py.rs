//! Askama template for a Python entity module.
//!
//! The generated module is self-contained: it builds on the canonical
//! `mududb` guest facade (`mududb.db.Database`, `mududb.sql.Params`/
//! `SqlStmt`, `mududb.result.Row` and the `as_*` scalar decoders). Per
//! table it emits the record dataclass with column/SQL constants, the
//! `Params` insert-params encoder, the `Row` decoder and — for tables with
//! a primary key — `get_by_pk`/`delete_by_pk` helpers.

use crate::entity::entity_info::EntityInfo;
use crate::lang_impl::lang::lang_kind::LangKind;
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::table::table_def::TableDef;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

/// Per-field rendering data for one entity column.
pub struct EntityFieldPy {
    /// Snake-case field name.
    pub name: String,
    /// Upper-snake column constant name.
    pub const_name: String,
    /// Raw column name.
    pub column_name: String,
    /// Python field annotation (`int`, `Optional[str]`).
    pub field_type: String,
    /// Dataclass default-value expression.
    pub default_value: String,
    /// Field-typed decode expression from a `Row` in scope as `row`.
    pub decode_expr: String,
}

/// Primary-key parameter of the `get_by_pk`/`delete_by_pk` helpers.
pub struct EntityPkPy {
    /// Snake-case parameter name.
    pub name: String,
    /// Bare Python parameter annotation (primary keys are never null).
    pub param_type: String,
}

/// Askama template for a Python entity module.
#[derive(Template)]
#[template(path = "python/entity.py.jinja", escape = "none")]
pub struct TemplateEntityPy {
    /// Entity metadata used by the template.
    pub table: EntityInfo,
    /// Per-field rendering data, in column declaration order.
    pub fields: Vec<EntityFieldPy>,
    /// Primary-key helper parameters (empty when the table has no primary key).
    pub pk_params: Vec<EntityPkPy>,
    /// Names of the `mududb.result` decoders the module imports.
    pub result_helpers: Vec<String>,
    /// `true` when a nullable column requires `Optional`.
    pub needs_optional: bool,
    /// `true` when a string-like non-string column (numeric, date/time)
    /// requires the lenient `_datum_text` decoder.
    pub needs_text_helper: bool,
}

impl TemplateEntityPy {
    /// Build the template from a table schema.
    pub fn from_table_schema(table_schema: &TableDef) -> RS<Self> {
        let table = EntityInfo::from_record_def(table_schema, &LangKind::Python)?;
        let mut fields = Vec::with_capacity(table_schema.table_columns().len());
        let mut pk_params = Vec::new();
        let mut helpers = Vec::new();
        let mut needs_optional = false;
        let mut needs_text_helper = false;
        for (field, column) in table.fields.iter().zip(table_schema.table_columns().iter()) {
            let scalar = scalar_of(column.data_type())?;
            if field.is_nullable {
                needs_optional = true;
            }
            let helper = decode_helper(scalar);
            if helper == "_datum_text" {
                needs_text_helper = true;
            } else if !helpers.contains(&helper) {
                helpers.push(helper);
            }
            fields.push(EntityFieldPy::from_column(
                field,
                scalar,
                table.struct_obj_name.as_str(),
            ));
            if field.is_primary_key {
                pk_params.push(EntityPkPy {
                    name: field.field_name_snake_case.clone(),
                    param_type: field.data_type.clone(),
                });
            }
        }
        helpers.sort();
        // `get_by_pk` returns `Optional[<record>]` whenever a primary key
        // exists, even if no column is nullable.
        let needs_optional = needs_optional || table.has_primary_key;
        Ok(Self {
            table,
            fields,
            pk_params,
            result_helpers: helpers,
            needs_optional,
            needs_text_helper,
        })
    }
}

impl EntityFieldPy {
    fn from_column(
        field: &crate::entity::field_info::FieldInfo,
        scalar: &UniScalar,
        struct_name: &str,
    ) -> Self {
        let helper = decode_helper(scalar);
        let row_value = format!(
            "row.value_by_name({struct_name}.{})",
            field.field_name_const
        );
        let (field_type, default_value, decode_expr) = if field.is_not_null {
            (
                field.data_type.clone(),
                default_value_expr(scalar),
                format!("{helper}({row_value})"),
            )
        } else {
            (
                format!("Optional[{}]", field.data_type),
                "None".to_string(),
                format!(
                    "None if row.is_null_by_name({struct_name}.{}) else {helper}({row_value})",
                    field.field_name_const
                ),
            )
        };
        Self {
            name: field.field_name_snake_case.clone(),
            const_name: field.field_name_const.clone(),
            column_name: field.field_name.clone(),
            field_type,
            default_value,
            decode_expr,
        }
    }
}

/// The column type must be a transportable scalar: table columns never map
/// to composite universal types from DDL, named (identifier) columns have
/// no wire mapping in the guest facade, and i128/u128 decode to a 16-byte
/// scalar payload the facade's `as_i64` does not unwrap.
fn scalar_of(data_type: &UniDataType) -> RS<&UniScalar> {
    let UniDataType::Scalar(scalar) = data_type else {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!(
                "entity code generation for Python only supports scalar column \
                 types, got {data_type:?}"
            )
        ));
    };
    match scalar {
        UniScalar::I128 | UniScalar::U128 => Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!(
                "entity code generation for Python does not support {scalar:?} \
                 columns (the guest facade cannot transport 128-bit integers)"
            )
        )),
        _ => Ok(scalar),
    }
}

/// The `mududb.result` decoder for a scalar; string-like scalars other than
/// `String` (numeric, date/time kinds, char) decode through the lenient
/// module-local `_datum_text` helper instead.
fn decode_helper(scalar: &UniScalar) -> String {
    match scalar {
        UniScalar::Bool => "as_bool",
        UniScalar::U8
        | UniScalar::U16
        | UniScalar::U32
        | UniScalar::U64
        | UniScalar::I8
        | UniScalar::I16
        | UniScalar::I32
        | UniScalar::I64 => "as_i64",
        UniScalar::F32 | UniScalar::F64 => "as_f64",
        UniScalar::String => "as_string",
        UniScalar::Blob => "as_bytes",
        UniScalar::Char
        | UniScalar::Numeric
        | UniScalar::Date
        | UniScalar::Time
        | UniScalar::Timestamp
        | UniScalar::TimestampTz => "_datum_text",
        UniScalar::I128 | UniScalar::U128 => {
            unreachable!("128-bit integers rejected by scalar_of")
        }
    }
    .to_string()
}

/// Dataclass default for a `NOT NULL` column.
fn default_value_expr(scalar: &UniScalar) -> String {
    match scalar {
        UniScalar::Bool => "False".to_string(),
        UniScalar::U8
        | UniScalar::U16
        | UniScalar::U32
        | UniScalar::U64
        | UniScalar::I8
        | UniScalar::I16
        | UniScalar::I32
        | UniScalar::I64 => "0".to_string(),
        UniScalar::F32 | UniScalar::F64 => "0.0".to_string(),
        UniScalar::Char
        | UniScalar::String
        | UniScalar::Numeric
        | UniScalar::Date
        | UniScalar::Time
        | UniScalar::Timestamp
        | UniScalar::TimestampTz => "\"\"".to_string(),
        UniScalar::Blob => "b\"\"".to_string(),
        UniScalar::I128 | UniScalar::U128 => {
            unreachable!("128-bit integers rejected by scalar_of")
        }
    }
}

#[cfg(test)]
#[path = "template_entity_py_test.rs"]
mod template_entity_py_test;
