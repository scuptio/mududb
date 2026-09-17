//! Unit tests for the C# entity template.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use super::TemplateEntityCS;
use askama::Template;
use mudu::error::ErrorCode;
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
    let template = TemplateEntityCS::from_table_schema(&sample_table_def()).unwrap();
    let rendered = template.render().unwrap();

    assert!(rendered.contains("#nullable enable"));
    assert!(rendered.contains("internal sealed class Items"));
    assert!(rendered.contains("public const string TableName = \"items\";"));
    assert!(rendered.contains("public const string ItemId = \"item_id\";"));
    assert!(rendered.contains(
        "public const string SqlInsert = \
         \"INSERT INTO items (item_id, name, quantity, price) VALUES (?, ?, ?, ?)\";"
    ));
    assert!(rendered.contains(
        "public const string SqlGetByPk = \
         \"SELECT item_id, name, quantity, price FROM items WHERE item_id = ?\";"
    ));
    assert!(rendered.contains(
        "public const string SqlDeleteByPk = \
         \"DELETE FROM items WHERE item_id = ?\";"
    ));
    assert!(rendered.contains("public int ItemId { get; set; }"));
    assert!(rendered.contains("public string? Name { get; set; }"));
    assert!(rendered.contains("public int? Quantity { get; set; }"));
    assert!(rendered.contains("public double Price { get; set; }"));
}

#[test]
fn renders_insert_params_and_row_decode() {
    let template = TemplateEntityCS::from_table_schema(&sample_table_def()).unwrap();
    let rendered = template.render().unwrap();

    assert!(rendered.contains("public object?[] InsertParams() =>"));
    assert!(rendered.contains("this.ItemId,"));
    assert!(rendered.contains("this.Name,"));
    assert!(rendered.contains("this.Quantity,"));
    assert!(rendered.contains("public static Items FromRow(object?[] row) =>"));
    // Integers arrive as `long`; string-likes keep null through a plain cast.
    assert!(rendered.contains("ItemId = (int)(long)row[0]!,"));
    assert!(rendered.contains("Name = (string?)row[1],"));
    assert!(rendered.contains("Quantity = row[2] is null ? (int?)null : (int)(long)row[2]!,"));
    assert!(rendered.contains("Price = (double)row[3]!,"));
}

#[test]
fn widens_f32_to_double() {
    let table = TableDef::new(
        "samples".to_string(),
        vec![
            ColumnDef::new(
                "sample_id".to_string(),
                UniDataType::Scalar(UniScalar::I64),
                None,
                true,
                true,
            ),
            ColumnDef::new(
                "ratio".to_string(),
                UniDataType::Scalar(UniScalar::F32),
                None,
                false,
                false,
            ),
        ],
    );
    let template = TemplateEntityCS::from_table_schema(&table).unwrap();
    let rendered = template.render().unwrap();

    assert!(rendered.contains("public float? Ratio { get; set; }"));
    assert!(rendered.contains("this.Ratio is null ? (double?)null : (double)this.Ratio.Value,"));
    assert!(rendered.contains("Ratio = row[1] is null ? (float?)null : (float)(double)row[1]!,"));
}

#[test]
fn omits_pk_members_when_table_has_no_primary_key() {
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
    let template = TemplateEntityCS::from_table_schema(&table).unwrap();
    let rendered = template.render().unwrap();

    assert!(!rendered.contains("SqlGetByPk"));
    assert!(!rendered.contains("SqlDeleteByPk"));
    assert!(rendered.contains("public const string SqlInsert ="));
}

#[test]
fn rejects_columns_the_sql_layer_cannot_transport() {
    for (name, ty) in [
        ("flag", UniDataType::Scalar(UniScalar::Bool)),
        ("huge", UniDataType::Scalar(UniScalar::I128)),
        ("named", UniDataType::Identifier("photo_fs".to_string())),
    ] {
        let table = TableDef::new(
            "t".to_string(),
            vec![ColumnDef::new(name.to_string(), ty, None, true, false)],
        );
        let err = TemplateEntityCS::from_table_schema(&table)
            .err()
            .expect("unsupported columns must be rejected");
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    }
}
