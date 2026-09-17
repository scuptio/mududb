//! Askama template for a C# entity source file.
//!
//! The generated file is self-contained in the global namespace (mgen does
//! not know the scaffold project's namespace): the record class with
//! column/SQL constants, the `object?[]` insert-params encoder matching the
//! `MuduSys.Command`/`MuduSys.Query` wire shapes, and the `FromRow` decoder
//! for rows selected in column declaration order. Executing the statements
//! stays with the caller because `MuduSys`/`MuduOid` live in the project
//! namespace, which the generated file cannot reference.
//!
//! Only the scalar set the `MuduSys` SQL layer can transport is supported:
//! `int`/`long` (I32/I64), `double` (F64; F32 is widened to `double`),
//! `string` (the string-like scalars) and `byte[]` (Blob). Other scalars
//! (bool, the small/unsigned ints, i128/u128, char) have no case in the
//! `MuduSys` parameter encoder and are rejected at generation time.

use crate::entity::entity_info::EntityInfo;
use crate::lang_impl::lang::lang_kind::LangKind;
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::table::table_def::TableDef;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

/// Per-field rendering data for one entity column.
pub struct EntityFieldCS {
    /// PascalCase property name.
    pub property_name: String,
    /// Raw column name.
    pub column_name: String,
    /// C# property type (`int`, `string?`, `int?`).
    pub field_type: String,
    /// Element expression of the `object?[]` insert-params array from
    /// `this.<Property>`.
    pub encode_expr: String,
    /// Field-typed decode expression from `row[<index>]`.
    pub decode_expr: String,
}

/// Askama template for a C# entity source file.
#[derive(Template)]
#[template(path = "csharp/entity.cs.jinja", escape = "none")]
pub struct TemplateEntityCS {
    /// Entity metadata used by the template.
    pub table: EntityInfo,
    /// Per-field rendering data, in column declaration order.
    pub fields: Vec<EntityFieldCS>,
}

impl TemplateEntityCS {
    /// Build the template from a table schema.
    pub fn from_table_schema(table_schema: &TableDef) -> RS<Self> {
        let table = EntityInfo::from_record_def(table_schema, &LangKind::CSharp)?;
        let mut fields = Vec::with_capacity(table_schema.table_columns().len());
        for (index, (field, column)) in table
            .fields
            .iter()
            .zip(table_schema.table_columns().iter())
            .enumerate()
        {
            let scalar = scalar_of(column.data_type())?;
            fields.push(EntityFieldCS::from_column(field, scalar, index));
        }
        Ok(Self { table, fields })
    }
}

impl EntityFieldCS {
    fn from_column(
        field: &crate::entity::field_info::FieldInfo,
        scalar: &UniScalar,
        index: usize,
    ) -> Self {
        let property_name = to_pascal_case(&field.field_name);
        let this = format!("this.{property_name}");
        let row_value = format!("row[{index}]");
        let (field_type, encode_expr, decode_expr) = if field.is_not_null {
            (
                field.data_type.clone(),
                encode_value_expr(scalar, &this, false),
                decode_value_expr(scalar, &row_value, false),
            )
        } else {
            (
                format!("{}?", field.data_type),
                encode_value_expr(scalar, &this, true),
                decode_value_expr(scalar, &row_value, true),
            )
        };
        Self {
            property_name,
            column_name: field.field_name.clone(),
            field_type,
            encode_expr,
            decode_expr,
        }
    }
}

/// The column type must be a scalar the `MuduSys` SQL layer transports:
/// table columns never map to composite universal types from DDL, named
/// (identifier) columns have no wire mapping, and several scalars (bool,
/// the small/unsigned ints, i128/u128, char) have no case in the `MuduSys`
/// parameter encoder.
fn scalar_of(data_type: &UniDataType) -> RS<&UniScalar> {
    let UniDataType::Scalar(scalar) = data_type else {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!(
                "entity code generation for C# only supports scalar column \
                 types, got {data_type:?}"
            )
        ));
    };
    match scalar {
        UniScalar::I32
        | UniScalar::I64
        | UniScalar::F32
        | UniScalar::F64
        | UniScalar::String
        | UniScalar::Numeric
        | UniScalar::Date
        | UniScalar::Time
        | UniScalar::Timestamp
        | UniScalar::TimestampTz
        | UniScalar::Blob => Ok(scalar),
        other => Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!(
                "entity code generation for C# does not support {other:?} \
                 columns (the MuduSys SQL layer cannot transport them)"
            )
        )),
    }
}

/// Element expression of the `object?[]` insert-params array. Values box
/// directly; `MuduSys` encodes `int` as I32, `long` as I64, `double` as F64,
/// `string` as String, `byte[]` as Blob and `null` as Null. `float` has no
/// `MuduSys` case, so it is widened to `double`.
fn encode_value_expr(scalar: &UniScalar, access: &str, nullable: bool) -> String {
    match scalar {
        UniScalar::F32 => {
            if nullable {
                format!("{access} is null ? (double?)null : (double){access}.Value")
            } else {
                format!("(double){access}")
            }
        }
        _ => access.to_string(),
    }
}

/// Decode `row[<index>]` into the field type. `MuduSys.Query` decodes every
/// integer scalar to `long`, every float to `double`, the string-like
/// scalars to `string`, blobs to `byte[]` and SQL NULL to `null`.
fn decode_value_expr(scalar: &UniScalar, access: &str, nullable: bool) -> String {
    let cast = match scalar {
        UniScalar::I32 => format!("(int)(long){access}!"),
        UniScalar::I64 => format!("(long){access}!"),
        UniScalar::F32 => format!("(float)(double){access}!"),
        UniScalar::F64 => format!("(double){access}!"),
        UniScalar::String
        | UniScalar::Numeric
        | UniScalar::Date
        | UniScalar::Time
        | UniScalar::Timestamp
        | UniScalar::TimestampTz => format!("(string){access}!"),
        UniScalar::Blob => format!("(byte[]){access}!"),
        other => unreachable!("unsupported scalar {other:?} rejected by scalar_of"),
    };
    if !nullable {
        return cast;
    }
    match scalar {
        UniScalar::I32 | UniScalar::I64 | UniScalar::F32 | UniScalar::F64 => {
            let value_type = match scalar {
                UniScalar::I32 => "int?",
                UniScalar::I64 => "long?",
                UniScalar::F32 => "float?",
                _ => "double?",
            };
            format!("{access} is null ? ({value_type})null : {cast}")
        }
        // Reference types keep null through a plain cast.
        UniScalar::String
        | UniScalar::Numeric
        | UniScalar::Date
        | UniScalar::Time
        | UniScalar::Timestamp
        | UniScalar::TimestampTz => format!("(string?){access}"),
        UniScalar::Blob => format!("(byte[]?){access}"),
        other => unreachable!("unsupported scalar {other:?} rejected by scalar_of"),
    }
}

#[cfg(test)]
#[path = "template_entity_cs_test.rs"]
mod template_entity_cs_test;
