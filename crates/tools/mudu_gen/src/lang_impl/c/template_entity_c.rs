//! Askama template for a C entity header (`<table>.h`).
//!
//! The generated header is self-contained: the row struct plus
//! `static inline` codecs and typed SQL helpers over the `mudu_sys` layer
//! of the mpm-crate C guest. The template is dumb — every expression it
//! emits is precomputed here so the logic stays unit-testable.

use crate::entity::entity_info::EntityInfo;
use crate::lang_impl::lang::lang_kind::LangKind;
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::{to_snake_case, to_snake_case_upper};
use mudu_binding::table::column_def::ColumnDef;
use mudu_binding::table::table_def::TableDef;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

/// Askama template for a C entity header.
#[derive(Template)]
#[template(path = "c/entity.h.jinja", escape = "none")]
pub struct TemplateEntityC {
    /// Entity view model used by the template.
    table: CEntity,
}

impl TemplateEntityC {
    /// Build the template from a table schema.
    pub fn from_table_schema(table_schema: &TableDef) -> RS<Self> {
        let info = EntityInfo::from_record_def(table_schema, &LangKind::C)?;
        Ok(Self {
            table: CEntity::from_info(&info, table_schema)?,
        })
    }
}

/// The datum shape a column rides on in the C guest wire layer.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum CWireKind {
    /// Integer scalars (and Bool as 0/1) travel as `MUDU_DATUM_I64`.
    Int,
    /// Boolean scalar; encoded as `MUDU_DATUM_I64` 0/1, decoded nonzero.
    Bool,
    /// Float scalars travel as `MUDU_DATUM_F64`.
    Double,
    /// String-family scalars travel as `MUDU_DATUM_STR` byte slices.
    Str,
    /// A `char` field encoded as a 1-byte `MUDU_DATUM_STR`.
    Char,
}

/// Classify a column type into its wire shape; `None` when the C guest
/// datum layer cannot carry the type (blobs, 128-bit integers, and every
/// non-scalar shape).
fn c_wire_kind(data_type: &UniDataType) -> Option<CWireKind> {
    match data_type {
        UniDataType::Scalar(scalar) => match scalar {
            UniScalar::Bool => Some(CWireKind::Bool),
            UniScalar::U8
            | UniScalar::U16
            | UniScalar::U32
            | UniScalar::U64
            | UniScalar::I8
            | UniScalar::I16
            | UniScalar::I32
            | UniScalar::I64 => Some(CWireKind::Int),
            UniScalar::F32 | UniScalar::F64 => Some(CWireKind::Double),
            UniScalar::Char => Some(CWireKind::Char),
            UniScalar::String
            | UniScalar::Numeric
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => Some(CWireKind::Str),
            // No blob or 128-bit datum shape in the C guest wire layer.
            UniScalar::Blob | UniScalar::U128 | UniScalar::I128 => None,
        },
        _ => None,
    }
}

/// Template view of one entity: precomputed names, SQL text and fields.
#[derive(Debug)]
struct CEntity {
    /// Raw table name (`items`).
    entity_name: String,
    /// Snake-case table name; prefixes the generated functions.
    snake_name: String,
    /// Include guard macro (`MUDU_ENTITY_ITEMS_H`).
    guard: String,
    /// Upper-snake table name; prefixes the generated macros (`ITEMS_`).
    macro_prefix: String,
    /// Row struct name (`items_row`).
    struct_name: String,
    /// Number of columns.
    column_count: usize,
    /// Comma-separated column names in declaration order.
    column_name_list: String,
    /// Comma-separated `?` placeholders, one per column.
    placeholder_list: String,
    /// Primary-key predicate (`"a = ?"`); empty without a primary key.
    pk_predicate: String,
    /// `true` when the table declares a primary key.
    has_primary_key: bool,
    /// All fields in column declaration order.
    fields: Vec<CField>,
    /// Primary-key fields in column declaration order.
    pk_fields: Vec<CField>,
}

/// Template view of one column: declaration plus precomputed codec text.
#[derive(Debug)]
struct CField {
    /// Column index in declaration order.
    index: usize,
    /// Snake-case field name.
    name: String,
    /// Raw column name (for comments).
    column_name: String,
    /// C declaration type (`int64_t`, `double`, `mudu_string`, `char`, `bool`).
    decl_type: String,
    /// `true` when the column accepts SQL NULL (a `<name>_is_null` flag is
    /// generated next to the value field).
    is_nullable: bool,
    /// Datum kind the column decodes from (`MUDU_DATUM_I64`, ...).
    expected_kind: &'static str,
    /// Datum expression over `row-><name>` for the non-NULL encode case.
    encode_expr: String,
    /// Statement(s) assigning `dst-><name>` from the kind-checked datum
    /// `f[<index>]`; may contain embedded newlines (indentation included).
    decode_assign: String,
    /// Datum expression over a by-value function parameter named `<name>`
    /// (used by the primary-key helpers).
    pk_encode_expr: String,
    /// `true` when the column is part of the primary key.
    is_pk: bool,
}

impl CEntity {
    fn from_info(info: &EntityInfo, table_schema: &TableDef) -> RS<Self> {
        let mut fields = Vec::with_capacity(info.fields.len());
        for (index, (field, column)) in info
            .fields
            .iter()
            .zip(table_schema.table_columns().iter())
            .enumerate()
        {
            fields.push(CField::from_info(index, field, column)?);
        }
        let pk_fields = fields
            .iter()
            .filter(|f| f.is_pk)
            .map(|f| f.clone_shallow())
            .collect::<Vec<_>>();
        let snake_name = to_snake_case(&info.entity_name);
        let macro_prefix = to_snake_case_upper(&info.entity_name);
        Ok(Self {
            entity_name: info.entity_name.clone(),
            guard: format!("MUDU_ENTITY_{macro_prefix}_H"),
            struct_name: format!("{snake_name}_row"),
            snake_name,
            macro_prefix,
            column_count: info.fields.len(),
            column_name_list: info.column_name_list.clone(),
            placeholder_list: info.placeholder_list.clone(),
            pk_predicate: info.pk_predicate.clone(),
            has_primary_key: info.has_primary_key,
            fields,
            pk_fields,
        })
    }
}

impl CField {
    fn from_info(
        index: usize,
        field: &crate::entity::field_info::FieldInfo,
        column: &ColumnDef,
    ) -> RS<Self> {
        let wire_kind = c_wire_kind(column.data_type()).ok_or_else(|| {
            mudu_error!(
                ErrorCode::NotImplemented,
                format!(
                    "column '{}' of type {} is not supported by the C entity generator",
                    field.field_name, field.data_type
                )
            )
        })?;
        let name = &field.field_name_snake_case;
        let (expected_kind, encode_expr, decode_assign, pk_encode_expr) = match wire_kind {
            CWireKind::Int => (
                "MUDU_DATUM_I64",
                format!("mudu_i64(row->{name})"),
                format!("dst->{name} = f[{index}].i64;"),
                format!("mudu_i64({name})"),
            ),
            CWireKind::Bool => (
                "MUDU_DATUM_I64",
                format!("mudu_i64(row->{name} ? 1 : 0)"),
                format!("dst->{name} = f[{index}].i64 != 0;"),
                format!("mudu_i64({name} ? 1 : 0)"),
            ),
            CWireKind::Double => (
                "MUDU_DATUM_F64",
                format!("mudu_f64(row->{name})"),
                format!("dst->{name} = f[{index}].f64;"),
                format!("mudu_f64({name})"),
            ),
            CWireKind::Str => (
                "MUDU_DATUM_STR",
                format!("mudu_str(row->{name}.data, row->{name}.len)"),
                format!(
                    "dst->{name}.data = f[{index}].str;\n        dst->{name}.len = f[{index}].str_len;"
                ),
                format!("mudu_str({name}.data, {name}.len)"),
            ),
            CWireKind::Char => (
                "MUDU_DATUM_STR",
                format!("mudu_str(&row->{name}, 1)"),
                format!("dst->{name} = f[{index}].str_len > 0 ? f[{index}].str[0] : '\\0';"),
                format!("mudu_str(&{name}, 1)"),
            ),
        };
        Ok(Self {
            index,
            name: name.clone(),
            column_name: field.field_name.clone(),
            decl_type: field.data_type.clone(),
            is_nullable: field.is_nullable,
            expected_kind,
            encode_expr,
            decode_assign,
            pk_encode_expr,
            is_pk: field.is_primary_key,
        })
    }

    fn clone_shallow(&self) -> Self {
        Self {
            index: self.index,
            name: self.name.clone(),
            column_name: self.column_name.clone(),
            decl_type: self.decl_type.clone(),
            is_nullable: self.is_nullable,
            expected_kind: self.expected_kind,
            encode_expr: self.encode_expr.clone(),
            decode_assign: self.decode_assign.clone(),
            pk_encode_expr: self.pk_encode_expr.clone(),
            is_pk: self.is_pk,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TemplateEntityC, c_wire_kind};
    use askama::Template;
    use mudu::common::result::RS;
    use mudu::error::ErrorCode;
    use mudu_binding::table::column_def::ColumnDef;
    use mudu_binding::table::table_def::TableDef;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_scalar::UniScalar;
    use sql_parser::parser::ddl_parser::DDLParser;

    fn render_ddl(sql: &str) -> RS<String> {
        let parser = DDLParser::new()?;
        let tables = parser.parse(sql)?;
        let mut out = String::new();
        for table in &tables {
            let template = TemplateEntityC::from_table_schema(table)?;
            let rendered = template
                .render()
                .map_err(|e| mudu::mudu_error!(ErrorCode::Decode, "render error", e))?;
            out.push_str(&rendered);
        }
        Ok(out)
    }

    #[test]
    fn renders_items_header() -> RS<()> {
        let sql = "CREATE TABLE items(item_id INT PRIMARY KEY, name VARCHAR(100), quantity INT);";
        let header = render_ddl(sql)?;
        assert!(header.contains("#ifndef MUDU_ENTITY_ITEMS_H"));
        assert!(header.contains("#include \"mudu_sys.h\""));
        assert!(header.contains("#define ITEMS_TABLE_NAME \"items\""));
        assert!(header.contains(
            "#define ITEMS_SQL_INSERT \"INSERT INTO items (item_id, name, quantity) VALUES (?, ?, ?)\""
        ));
        assert!(header.contains(
            "#define ITEMS_SQL_GET_BY_PK \"SELECT item_id, name, quantity FROM items WHERE item_id = ?\""
        ));
        assert!(header.contains("} items_row;"));
        assert!(header.contains("int64_t item_id;"));
        assert!(header.contains("mudu_string name;"));
        assert!(header.contains("bool name_is_null;"));
        assert!(header.contains("bool quantity_is_null;"));
        assert!(header.contains("static inline void items_row_init(items_row *row)"));
        assert!(header.contains(
            "out[1] = row->name_is_null ? mudu_null() : mudu_str(row->name.data, row->name.len);"
        ));
        assert!(header.contains("static inline int items_insert(mudu_oid session"));
        assert!(header.contains("static inline int items_get_by_pk(mudu_oid session,"));
        assert!(header.contains("static inline int items_delete_by_pk(mudu_oid session,"));
        Ok(())
    }

    #[test]
    fn table_without_primary_key_has_no_pk_helpers() -> RS<()> {
        let header = render_ddl("CREATE TABLE log(id INT, msg TEXT);")?;
        assert!(!header.contains("SQL_GET_BY_PK"));
        assert!(!header.contains("log_get_by_pk"));
        assert!(header.contains("static inline int log_insert(mudu_oid session"));
        Ok(())
    }

    #[test]
    fn rejects_columns_without_a_wire_shape() -> RS<()> {
        // No blob datum kind exists in the C guest wire layer.
        assert_eq!(c_wire_kind(&UniDataType::Scalar(UniScalar::Blob)), None);
        assert_eq!(c_wire_kind(&UniDataType::Scalar(UniScalar::U128)), None);
        assert_eq!(c_wire_kind(&UniDataType::Scalar(UniScalar::I128)), None);
        assert_eq!(
            c_wire_kind(&UniDataType::Array(Box::new(UniDataType::Scalar(
                UniScalar::I32
            )))),
            None
        );

        let table = TableDef::new(
            "t".to_string(),
            vec![
                ColumnDef::new(
                    "id".to_string(),
                    UniDataType::Scalar(UniScalar::I32),
                    None,
                    true,
                    true,
                ),
                ColumnDef::new(
                    "data".to_string(),
                    UniDataType::Scalar(UniScalar::Blob),
                    None,
                    false,
                    false,
                ),
            ],
        );
        match TemplateEntityC::from_table_schema(&table) {
            Ok(_) => Err(mudu::mudu_error!(
                ErrorCode::Internal,
                "expected blob column to be rejected"
            )),
            Err(err) => {
                assert_eq!(err.ec(), ErrorCode::NotImplemented);
                Ok(())
            }
        }
    }

    #[test]
    fn rejects_ddl_hugeint_columns() -> RS<()> {
        // HUGEINT is I128; the C guest wire layer has no 128-bit shape.
        match render_ddl("CREATE TABLE t(id INT PRIMARY KEY, big HUGEINT);") {
            Ok(_) => Err(mudu::mudu_error!(
                ErrorCode::Internal,
                "expected HUGEINT column to be rejected"
            )),
            Err(err) => {
                assert_eq!(err.ec(), ErrorCode::NotImplemented);
                Ok(())
            }
        }
    }

    #[test]
    fn rendering_is_deterministic() -> RS<()> {
        let sql = "CREATE TABLE items(item_id INT PRIMARY KEY, name VARCHAR(100), quantity INT);";
        assert_eq!(render_ddl(sql)?, render_ddl(sql)?);
        Ok(())
    }
}
