//! Unit tests for the AssemblyScript entity template.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use super::TemplateEntityAS;
use askama::Template;
use mudu_binding::table::column_def::ColumnDef;
use mudu_binding::table::table_def::TableDef;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

fn sample_table_def() -> TableDef {
    TableDef::new(
        "items".to_string(),
        vec![
            ColumnDef::new(
                "item_id".to_string(),
                UniDataType::Scalar(UniScalar::I32),
                None,
                true,
                true,
            ),
            ColumnDef::new(
                "name".to_string(),
                UniDataType::Scalar(UniScalar::String),
                None,
                false,
                false,
            ),
            ColumnDef::new(
                "quantity".to_string(),
                UniDataType::Scalar(UniScalar::I32),
                None,
                false,
                false,
            ),
            ColumnDef::new(
                "price".to_string(),
                UniDataType::Scalar(UniScalar::F64),
                None,
                true,
                false,
            ),
        ],
    )
}

#[test]
fn renders_record_with_sql_constants() {
    let template = TemplateEntityAS::from_table_schema(&sample_table_def()).unwrap();
    let rendered = template.render().unwrap();

    assert!(rendered.contains("export class Items {"));
    assert!(rendered.contains("static readonly TABLE_NAME: string = \"items\";"));
    assert!(rendered.contains(
        "static readonly SQL_INSERT: string = \
         \"INSERT INTO items (item_id, name, quantity, price) VALUES (?, ?, ?, ?)\";"
    ));
    assert!(rendered.contains(
        "static readonly SQL_GET_BY_PK: string = \
         \"SELECT item_id, name, quantity, price FROM items WHERE item_id = ?\";"
    ));
    assert!(rendered.contains(
        "static readonly SQL_DELETE_BY_PK: string = \
         \"DELETE FROM items WHERE item_id = ?\";"
    ));
    assert!(rendered.contains("item_id: i32;"));
    assert!(rendered.contains("price: f64;"));
}

#[test]
fn renders_nullable_fields() {
    let template = TemplateEntityAS::from_table_schema(&sample_table_def()).unwrap();
    let rendered = template.render().unwrap();

    // Nullable string columns stay `T | null`; nullable value-type columns
    // need the `Box<T>` wrapper (AssemblyScript value types cannot be
    // nullable), so the module declares the box class once.
    assert!(rendered.contains("export class Box<T> {"));
    assert!(rendered.contains("name: string | null;"));
    assert!(rendered.contains("quantity: Box<i32> | null;"));
    assert!(rendered.contains("this.name === null ? Value.null() : Value.text(this.name!)"));
    assert!(
        rendered
            .contains("this.quantity === null ? Value.null() : Value.int64(this.quantity!.value)")
    );
    assert!(rendered.contains(
        "row.isNullByName(Items.QUANTITY) ? null : \
         new Box<i32>(row.valueByName(Items.QUANTITY).asInt64() as i32)"
    ));
}

#[test]
fn renders_codec_and_crud_helpers() {
    let template = TemplateEntityAS::from_table_schema(&sample_table_def()).unwrap();
    let rendered = template.render().unwrap();

    assert!(rendered.contains("params.bind(0, Value.int64(this.item_id));"));
    assert!(rendered.contains("params.bind(3, Value.float64(this.price));"));
    assert!(rendered.contains("insert(id: Oid): u64 {"));
    assert!(rendered.contains("static fromRow(row: Row): Items {"));
    assert!(rendered.contains("row.valueByName(Items.ITEM_ID).asInt64() as i32"));
    assert!(rendered.contains("static getByPk("));
    assert!(rendered.contains("static deleteByPk("));
}

#[test]
fn omits_pk_helpers_and_box_when_not_needed() {
    let table = TableDef::new(
        "logs".to_string(),
        vec![ColumnDef::new(
            "message".to_string(),
            UniDataType::Scalar(UniScalar::String),
            None,
            true,
            false,
        )],
    );
    let template = TemplateEntityAS::from_table_schema(&table).unwrap();
    let rendered = template.render().unwrap();

    assert!(!rendered.contains("SQL_GET_BY_PK"));
    assert!(!rendered.contains("SQL_DELETE_BY_PK"));
    assert!(!rendered.contains("static getByPk("));
    assert!(!rendered.contains("export class Box<T> {"));
    assert!(rendered.contains("static readonly SQL_INSERT: string ="));
}

#[test]
fn rejects_non_scalar_columns() {
    let table = TableDef::new(
        "photos".to_string(),
        vec![ColumnDef::new(
            "blob".to_string(),
            UniDataType::Identifier("photo_fs".to_string()),
            None,
            true,
            false,
        )],
    );
    let err = TemplateEntityAS::from_table_schema(&table)
        .err()
        .expect("identifier columns must be rejected");
    assert_eq!(err.ec(), mudu::error::ErrorCode::NotImplemented);
}
