//! Askama template for an AssemblyScript entity module.
//!
//! The generated module is self-contained: it builds on the guest binding
//! facade (`@mududb/mududb`: `Value`/`ValueList`/`SqlStmt`/`Row` and the
//! `witCommand`/`witQuery` byte-pipe helpers), not on any trait system.
//! Per table it emits the record class with column/SQL constants, the
//! `ValueList` insert-params encoder, the `Row` decoder and — for tables
//! with a primary key — `getByPk`/`deleteByPk` helpers.
//!
//! AssemblyScript value types cannot be nullable, so a nullable column of a
//! value type (ints, floats, bool) is represented as `Box<T> | null` with a
//! small generic `Box<T>` class emitted into the module; nullable
//! reference-type columns (strings, blobs) stay `T | null`.

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
pub struct EntityFieldAS {
    /// Snake-case field name.
    pub name: String,
    /// Upper-snake column constant name.
    pub const_name: String,
    /// Raw column name.
    pub column_name: String,
    /// AssemblyScript field type (`i32`, `string | null`, `Box<i32> | null`).
    pub field_type: String,
    /// Constructor default-value expression.
    pub default_value: String,
    /// `Value`-producing encode expression from `this.<name>`.
    pub encode_expr: String,
    /// Field-typed decode expression from a `Row` in scope as `row`.
    pub decode_expr: String,
}

/// Primary-key parameter of the `getByPk`/`deleteByPk` helpers.
pub struct EntityPkAS {
    /// Snake-case parameter name.
    pub name: String,
    /// Bare AssemblyScript parameter type (primary keys are never null).
    pub param_type: String,
    /// `Value`-producing encode expression from the parameter variable.
    pub param_encode_expr: String,
}

/// Askama template for an AssemblyScript entity module.
#[derive(Template)]
#[template(path = "assemblyscript/entity.ts.jinja", escape = "none")]
pub struct TemplateEntityAS {
    /// Entity metadata used by the template.
    pub table: EntityInfo,
    /// Per-field rendering data, in column declaration order.
    pub fields: Vec<EntityFieldAS>,
    /// Primary-key helper parameters (empty when the table has no primary key).
    pub pk_params: Vec<EntityPkAS>,
    /// `true` when a nullable value-type column requires the `Box<T>` class.
    pub needs_box: bool,
}

impl TemplateEntityAS {
    /// Build the template from a table schema.
    pub fn from_table_schema(table_schema: &TableDef) -> RS<Self> {
        let table = EntityInfo::from_record_def(table_schema, &LangKind::AssemblyScript)?;
        let mut fields = Vec::with_capacity(table_schema.table_columns().len());
        let mut pk_params = Vec::new();
        let mut needs_box = false;
        for (field, column) in table.fields.iter().zip(table_schema.table_columns().iter()) {
            let scalar = scalar_of(column.data_type())?;
            let struct_name = table.struct_obj_name.as_str();
            let field_as = EntityFieldAS::from_column(field, scalar, struct_name, &mut needs_box);
            if field.is_primary_key {
                pk_params.push(EntityPkAS {
                    name: field.field_name_snake_case.clone(),
                    param_type: field.data_type.clone(),
                    param_encode_expr: to_value_expr(scalar, &field.field_name_snake_case),
                });
            }
            fields.push(field_as);
        }
        Ok(Self {
            table,
            fields,
            pk_params,
            needs_box,
        })
    }
}

impl EntityFieldAS {
    fn from_column(
        field: &crate::entity::field_info::FieldInfo,
        scalar: &UniScalar,
        struct_name: &str,
        needs_box: &mut bool,
    ) -> Self {
        let name = field.field_name_snake_case.clone();
        let this = format!("this.{name}");
        let row_value = format!("row.valueByName({struct_name}.{})", field.field_name_const);
        let row_is_null = format!("row.isNullByName({struct_name}.{})", field.field_name_const);
        let (field_type, default_value, encode_expr, decode_expr) = if field.is_not_null {
            (
                field.data_type.clone(),
                default_value_expr(scalar),
                to_value_expr(scalar, &this),
                from_value_expr(scalar, &row_value),
            )
        } else if is_value_scalar(scalar) {
            *needs_box = true;
            (
                format!("Box<{}> | null", field.data_type),
                "null".to_string(),
                format!(
                    "{this} === null ? Value.null() : {}",
                    to_value_expr(scalar, &format!("{this}!.value"))
                ),
                format!(
                    "{row_is_null} ? null : new Box<{}>({})",
                    field.data_type,
                    from_value_expr(scalar, &row_value)
                ),
            )
        } else {
            (
                format!("{} | null", field.data_type),
                "null".to_string(),
                format!(
                    "{this} === null ? Value.null() : {}",
                    to_value_expr(scalar, &format!("{this}!"))
                ),
                format!(
                    "{row_is_null} ? null : {}",
                    from_value_expr(scalar, &row_value)
                ),
            )
        };
        Self {
            name,
            const_name: field.field_name_const.clone(),
            column_name: field.field_name.clone(),
            field_type,
            default_value,
            encode_expr,
            decode_expr,
        }
    }
}

/// The column type must be a transportable scalar: table columns never map
/// to composite universal types from DDL, named (identifier) columns have
/// no `Value`-level wire mapping in the guest binding, and i128/u128 have no
/// AssemblyScript type to map to.
fn scalar_of(data_type: &UniDataType) -> RS<&UniScalar> {
    let UniDataType::Scalar(scalar) = data_type else {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!(
                "entity code generation for AssemblyScript only supports scalar \
                 column types, got {data_type:?}"
            )
        ));
    };
    match scalar {
        UniScalar::I128 | UniScalar::U128 => Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!(
                "entity code generation for AssemblyScript does not support \
                 {scalar:?} columns (AssemblyScript has no 128-bit integer type)"
            )
        )),
        _ => Ok(scalar),
    }
}

/// `true` for scalars whose AssemblyScript mapping is a value type (those
/// cannot be nullable and need the `Box<T>` wrapper when the column allows
/// SQL NULL).
fn is_value_scalar(scalar: &UniScalar) -> bool {
    matches!(
        scalar,
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
    )
}

/// Encode `access` (an lvalue of the field type) as a facade `Value`.
fn to_value_expr(scalar: &UniScalar, access: &str) -> String {
    match scalar {
        UniScalar::Bool => format!("Value.boolean({access})"),
        UniScalar::U64 => format!("Value.int64(<i64>{access})"),
        UniScalar::U8
        | UniScalar::U16
        | UniScalar::U32
        | UniScalar::I8
        | UniScalar::I16
        | UniScalar::I32
        | UniScalar::I64 => format!("Value.int64({access})"),
        UniScalar::F32 | UniScalar::F64 => format!("Value.float64({access})"),
        UniScalar::Char
        | UniScalar::String
        | UniScalar::Numeric
        | UniScalar::Date
        | UniScalar::Time
        | UniScalar::Timestamp
        | UniScalar::TimestampTz => format!("Value.text({access})"),
        UniScalar::Blob => format!("Value.binary({access})"),
        UniScalar::I128 | UniScalar::U128 => {
            unreachable!("128-bit integers rejected by scalar_of")
        }
    }
}

/// Decode `access` (a facade `Value`) into the field type.
fn from_value_expr(scalar: &UniScalar, access: &str) -> String {
    match scalar {
        UniScalar::Bool => format!("{access}.asBoolean()"),
        UniScalar::U8 => format!("{access}.asInt64() as u8"),
        UniScalar::U16 => format!("{access}.asInt64() as u16"),
        UniScalar::U32 => format!("{access}.asInt64() as u32"),
        UniScalar::U64 => format!("{access}.asInt64() as u64"),
        UniScalar::I8 => format!("{access}.asInt64() as i8"),
        UniScalar::I16 => format!("{access}.asInt64() as i16"),
        UniScalar::I32 => format!("{access}.asInt64() as i32"),
        UniScalar::I64 => format!("{access}.asInt64()"),
        UniScalar::F32 => format!("{access}.asFloat64() as f32"),
        UniScalar::F64 => format!("{access}.asFloat64()"),
        UniScalar::Char
        | UniScalar::String
        | UniScalar::Numeric
        | UniScalar::Date
        | UniScalar::Time
        | UniScalar::Timestamp
        | UniScalar::TimestampTz => format!("{access}.asText()"),
        UniScalar::Blob => format!("{access}.asBinary()"),
        UniScalar::I128 | UniScalar::U128 => {
            unreachable!("128-bit integers rejected by scalar_of")
        }
    }
}

/// Constructor default for a `NOT NULL` column.
fn default_value_expr(scalar: &UniScalar) -> String {
    match scalar {
        UniScalar::Bool => "false".to_string(),
        UniScalar::U8
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
        | UniScalar::F64 => "0".to_string(),
        UniScalar::Char
        | UniScalar::String
        | UniScalar::Numeric
        | UniScalar::Date
        | UniScalar::Time
        | UniScalar::Timestamp
        | UniScalar::TimestampTz => "\"\"".to_string(),
        UniScalar::Blob => "new Uint8Array(0)".to_string(),
    }
}

#[cfg(test)]
#[path = "template_entity_as_test.rs"]
mod template_entity_as_test;
