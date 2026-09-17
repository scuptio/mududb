//! Askama template for a Go entity file (`<table>.go`, package main).
//!
//! The generated file carries the row struct, the row codec and typed SQL
//! helpers over the syscall layer of the mpm-crate Go guest (`mudusys.go`).
//! The template is dumb — every expression it emits is precomputed here so
//! the logic stays unit-testable.

use crate::entity::entity_info::EntityInfo;
use crate::lang_impl::lang::lang_kind::LangKind;
use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::table::column_def::ColumnDef;
use mudu_binding::table::table_def::TableDef;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

/// Askama template for a Go entity file.
#[derive(Template)]
#[template(path = "go/entity.go.jinja", escape = "none")]
pub struct TemplateEntityGO {
    /// Entity view model used by the template.
    table: GoEntity,
}

impl TemplateEntityGO {
    /// Build the template from a table schema.
    pub fn from_table_schema(table_schema: &TableDef) -> RS<Self> {
        let info = EntityInfo::from_record_def(table_schema, &LangKind::Go)?;
        Ok(Self {
            table: GoEntity::from_info(&info, table_schema)?,
        })
    }
}

/// The datum shape a column rides on in the Go guest wire layer.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum GoWireKind {
    /// Integer scalars surface as `int64`.
    Int,
    /// Float scalars surface as `float64`.
    Float,
    /// String-family scalars (Char/String/Date/Time/Timestamp/TimestampTz)
    /// surface as `string`.
    Str,
    /// Boolean scalars surface as `bool`.
    Bool,
    /// Blob scalars surface as `[]byte` (record-context array of u8).
    Bytes,
    /// Numeric scalars surface as the hand-written `numeric` string type.
    Numeric,
}

/// Classify a column type into its wire shape; `None` when the Go guest
/// wire layer cannot carry the type (128-bit integers and every non-scalar
/// shape).
fn go_wire_kind(data_type: &UniDataType) -> Option<GoWireKind> {
    match data_type {
        UniDataType::Scalar(scalar) => match scalar {
            UniScalar::Bool => Some(GoWireKind::Bool),
            UniScalar::U8
            | UniScalar::U16
            | UniScalar::U32
            | UniScalar::U64
            | UniScalar::I8
            | UniScalar::I16
            | UniScalar::I32
            | UniScalar::I64 => Some(GoWireKind::Int),
            UniScalar::F32 | UniScalar::F64 => Some(GoWireKind::Float),
            UniScalar::Char
            | UniScalar::String
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => Some(GoWireKind::Str),
            UniScalar::Blob => Some(GoWireKind::Bytes),
            UniScalar::Numeric => Some(GoWireKind::Numeric),
            // No 128-bit datum shape in the Go guest wire layer.
            UniScalar::U128 | UniScalar::I128 => None,
        },
        // DDL `BLOB` arrives as `Binary` (record-context `list<u8>`).
        UniDataType::Binary => Some(GoWireKind::Bytes),
        _ => None,
    }
}

/// Lower-case the first character of a PascalCase identifier.
fn to_camel_ident(pascal: &str) -> String {
    let mut chars = pascal.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Template view of one entity: precomputed names, SQL text and fields.
#[derive(Debug)]
struct GoEntity {
    /// Raw table name (`items`).
    entity_name: String,
    /// PascalCase table name (`Items`).
    pascal_name: String,
    /// Row struct name (`ItemsRow`).
    struct_name: String,
    /// Camel-case table name; prefixes the generated constants
    /// (`itemsSQLInsert`).
    const_prefix: String,
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
    /// Primary-key parameter list (`itemID int64, name string`).
    pk_param_list: String,
    /// Primary-key argument list (`itemID, name`).
    pk_arg_list: String,
    /// All fields in column declaration order.
    fields: Vec<GoField>,
}

/// Template view of one column: declaration plus precomputed codec text.
#[derive(Debug)]
struct GoField {
    /// Column index in declaration order.
    index: usize,
    /// PascalCase struct field name (`ItemId`).
    name: String,
    /// Camel-case parameter name for the primary-key helpers (`itemId`).
    param_name: String,
    /// Non-pointer Go type (`int64`, `string`, `numeric`, ...).
    base_type: String,
    /// `true` when the column is part of the primary key.
    is_pk: bool,
    /// Raw column name (for comments and error messages).
    column_name: String,
    /// Go struct field type; a pointer type for nullable columns.
    decl_type: String,
    /// `true` when the column accepts SQL NULL (the field is a pointer).
    is_nullable: bool,
    /// Precomputed decode block for a non-nullable field (assertion plus
    /// assignment, tab-indented continuation lines included).
    decode_plain: String,
    /// Precomputed decode block for a nullable field (the else-arm of the
    /// `fields[i] == nil` check).
    decode_nested: String,
}

impl GoEntity {
    fn from_info(info: &EntityInfo, table_schema: &TableDef) -> RS<Self> {
        let mut fields = Vec::with_capacity(info.fields.len());
        for (index, (field, column)) in info
            .fields
            .iter()
            .zip(table_schema.table_columns().iter())
            .enumerate()
        {
            fields.push(GoField::from_info(index, field, column, &info.entity_name)?);
        }
        let pk_fields: Vec<&GoField> = fields.iter().filter(|f| f.is_pk).collect();
        let pk_param_list = pk_fields
            .iter()
            .map(|f| format!("{} {}", f.param_name, f.base_type))
            .collect::<Vec<_>>()
            .join(", ");
        let pk_arg_list = pk_fields
            .iter()
            .map(|f| f.param_name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        let pascal_name = info.struct_obj_name.clone();
        Ok(Self {
            entity_name: info.entity_name.clone(),
            struct_name: format!("{pascal_name}Row"),
            const_prefix: to_camel_ident(&pascal_name),
            pascal_name,
            column_count: info.fields.len(),
            column_name_list: info.column_name_list.clone(),
            placeholder_list: info.placeholder_list.clone(),
            pk_predicate: info.pk_predicate.clone(),
            has_primary_key: info.has_primary_key,
            pk_param_list,
            pk_arg_list,
            fields,
        })
    }
}

impl GoField {
    fn from_info(
        index: usize,
        field: &crate::entity::field_info::FieldInfo,
        column: &ColumnDef,
        table_name: &str,
    ) -> RS<Self> {
        let wire_kind = go_wire_kind(column.data_type()).ok_or_else(|| {
            mudu_error!(
                ErrorCode::NotImplemented,
                format!(
                    "column '{}' of type {} is not supported by the Go entity generator",
                    field.field_name, field.data_type
                )
            )
        })?;
        let base_type = match wire_kind {
            GoWireKind::Int => "int64",
            GoWireKind::Float => "float64",
            GoWireKind::Str => "string",
            GoWireKind::Bool => "bool",
            GoWireKind::Bytes => "[]byte",
            GoWireKind::Numeric => "numeric",
        };
        // The wire assertion type: Numeric travels as a string and is
        // wrapped after the assertion.
        let assert_type = match wire_kind {
            GoWireKind::Numeric => "string",
            _ => base_type,
        };
        let name = to_pascal_case(&field.field_name);
        let is_nullable = field.is_nullable;
        let decl_type = if is_nullable {
            format!("*{base_type}")
        } else {
            base_type.to_string()
        };
        let var = format!("v{index}");
        let convert = |v: &str| match wire_kind {
            GoWireKind::Numeric => format!("numeric({v})"),
            _ => v.to_string(),
        };
        let type_error = format!(
            "return fmt.Errorf(\"{table_name}: column {}: unexpected datum %T\", fields[{index}])",
            field.field_name
        );
        let decode_plain = format!(
            "{var}, ok := fields[{index}].({assert_type})\n\tif !ok {{\n\t\t{type_error}\n\t}}\n\tr.{name} = {}",
            convert(&var)
        );
        let assign_nested = match wire_kind {
            GoWireKind::Numeric => format!("nv := numeric(v)\n\t\tr.{name} = &nv"),
            _ => format!("r.{name} = &v"),
        };
        let decode_nested = format!(
            "v, ok := fields[{index}].({assert_type})\n\t\tif !ok {{\n\t\t\t{type_error}\n\t\t}}\n\t\t{assign_nested}"
        );
        Ok(Self {
            index,
            param_name: to_camel_ident(&name),
            base_type: base_type.to_string(),
            is_pk: field.is_primary_key,
            name,
            column_name: field.field_name.clone(),
            decl_type,
            is_nullable,
            decode_plain,
            decode_nested,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{TemplateEntityGO, go_wire_kind};
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
            let template = TemplateEntityGO::from_table_schema(table)?;
            let rendered = template
                .render()
                .map_err(|e| mudu::mudu_error!(ErrorCode::Decode, "render error", e))?;
            out.push_str(&rendered);
        }
        Ok(out)
    }

    #[test]
    fn renders_items_file() -> RS<()> {
        let sql = "CREATE TABLE items(item_id INT PRIMARY KEY, name VARCHAR(100), quantity INT);";
        let file = render_ddl(sql)?;
        assert!(file.contains("package main"));
        assert!(file.contains("itemsTableName = \"items\""));
        assert!(file.contains(
            "itemsSQLInsert = \"INSERT INTO items (item_id, name, quantity) VALUES (?, ?, ?)\""
        ));
        assert!(file.contains(
            "itemsSQLGetByPk = \"SELECT item_id, name, quantity FROM items WHERE item_id = ?\""
        ));
        assert!(file.contains("type ItemsRow struct {"));
        assert!(file.contains("ItemId int64"));
        assert!(file.contains("Name *string"));
        assert!(file.contains("Quantity *int64"));
        assert!(file.contains("func (r *ItemsRow) sqlArgs() []any {"));
        assert!(file.contains("func (r *ItemsRow) decodeFields(fields []any) error {"));
        assert!(file.contains("func (r *ItemsRow) Insert(session muduOid) (uint64, error) {"));
        assert!(
            file.contains("func GetItemsByPk(session muduOid, itemId int64) (*ItemsRow, error) {")
        );
        assert!(
            file.contains("func DeleteItemsByPk(session muduOid, itemId int64) (uint64, error) {")
        );
        // Nullable columns decode through the nil-check arm.
        assert!(file.contains("if fields[1] == nil {"));
        assert!(file.contains("r.Name = &v"));
        Ok(())
    }

    #[test]
    fn table_without_primary_key_has_no_pk_helpers() -> RS<()> {
        let file = render_ddl("CREATE TABLE log(id INT, msg TEXT);")?;
        assert!(!file.contains("SQLGetByPk"));
        assert!(!file.contains("GetLogByPk"));
        assert!(file.contains("func (r *LogRow) Insert(session muduOid) (uint64, error) {"));
        Ok(())
    }

    #[test]
    fn blob_and_numeric_columns_map_to_their_wire_shapes() -> RS<()> {
        // DDL has no BLOB keyword, so build the table definition by hand.
        let table = TableDef::new(
            "doc".to_string(),
            vec![
                ColumnDef::new(
                    "id".to_string(),
                    UniDataType::Scalar(UniScalar::I32),
                    None,
                    true,
                    true,
                ),
                ColumnDef::new(
                    "body".to_string(),
                    UniDataType::Scalar(UniScalar::Blob),
                    None,
                    true,
                    false,
                ),
                ColumnDef::new(
                    "price".to_string(),
                    UniDataType::Scalar(UniScalar::Numeric),
                    None,
                    false,
                    false,
                ),
            ],
        );
        let template = TemplateEntityGO::from_table_schema(&table)?;
        let file = template
            .render()
            .map_err(|e| mudu::mudu_error!(ErrorCode::Decode, "render error", e))?;
        assert!(file.contains("Body []byte"));
        assert!(file.contains("Price *numeric"));
        // Numeric travels as a string and is wrapped after the assertion.
        assert!(file.contains("nv := numeric(v)"));
        // Record-context `list<u8>` (UniDataType::Binary) rides the blob
        // wire shape as well.
        assert_eq!(
            go_wire_kind(&UniDataType::Binary),
            Some(super::GoWireKind::Bytes)
        );
        Ok(())
    }

    #[test]
    fn rejects_columns_without_a_wire_shape() -> RS<()> {
        assert_eq!(go_wire_kind(&UniDataType::Scalar(UniScalar::U128)), None);
        assert_eq!(go_wire_kind(&UniDataType::Scalar(UniScalar::I128)), None);
        assert_eq!(
            go_wire_kind(&UniDataType::Array(Box::new(UniDataType::Scalar(
                UniScalar::I32
            )))),
            None
        );

        let table = TableDef::new(
            "t".to_string(),
            vec![ColumnDef::new(
                "id".to_string(),
                UniDataType::Scalar(UniScalar::U128),
                None,
                true,
                true,
            )],
        );
        match TemplateEntityGO::from_table_schema(&table) {
            Ok(_) => Err(mudu::mudu_error!(
                ErrorCode::Internal,
                "expected u128 column to be rejected"
            )),
            Err(err) => {
                assert_eq!(err.ec(), ErrorCode::NotImplemented);
                Ok(())
            }
        }
    }

    #[test]
    fn rejects_ddl_hugeint_columns() -> RS<()> {
        // HUGEINT is I128; the Go guest wire layer has no 128-bit shape.
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
