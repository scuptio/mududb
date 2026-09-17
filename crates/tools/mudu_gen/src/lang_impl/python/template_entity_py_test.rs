//! Unit tests for the Python entity template.

#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use super::TemplateEntityPy;
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
                UniDataType::Scalar(UniScalar::Numeric),
                None,
                true,
                false,
            ),
        ],
    )
}

#[test]
fn renders_record_with_sql_constants() {
    let template = TemplateEntityPy::from_table_schema(&sample_table_def()).unwrap();
    let rendered = template.render().unwrap();

    assert!(rendered.contains("@dataclass"));
    assert!(rendered.contains("class Items:"));
    assert!(rendered.contains("TABLE_NAME: ClassVar[str] = \"items\""));
    assert!(rendered.contains("ITEM_ID: ClassVar[str] = \"item_id\""));
    assert!(rendered.contains(
        "SQL_INSERT: ClassVar[str] = \
         \"INSERT INTO items (item_id, name, quantity, price) VALUES (?, ?, ?, ?)\""
    ));
    assert!(rendered.contains(
        "SQL_GET_BY_PK: ClassVar[str] = \
         \"SELECT item_id, name, quantity, price FROM items WHERE item_id = ?\""
    ));
    assert!(rendered.contains(
        "SQL_DELETE_BY_PK: ClassVar[str] = \
         \"DELETE FROM items WHERE item_id = ?\""
    ));
    assert!(rendered.contains("item_id: int = 0"));
    assert!(rendered.contains("name: Optional[str] = None"));
    assert!(rendered.contains("quantity: Optional[int] = None"));
    assert!(rendered.contains("price: str = \"\""));
}

#[test]
fn renders_params_and_row_decode() {
    let template = TemplateEntityPy::from_table_schema(&sample_table_def()).unwrap();
    let rendered = template.render().unwrap();

    assert!(rendered.contains("from mududb.result import Row, as_i64, as_string"));
    assert!(rendered.contains(".bind(0, self.item_id)"));
    assert!(rendered.contains("def insert(self, db: Database) -> int:"));
    assert!(rendered.contains("def from_row(row: Row) -> \"Items\":"));
    assert!(rendered.contains("item_id=as_i64(row.value_by_name(Items.ITEM_ID)),"));
    assert!(rendered.contains(
        "name=None if row.is_null_by_name(Items.NAME) else \
         as_string(row.value_by_name(Items.NAME)),"
    ));
    // NUMERIC is a string-like scalar the facade's `as_string` does not
    // accept, so the module-local lenient text decoder is emitted and used.
    assert!(rendered.contains("def _datum_text(v) -> str:"));
    assert!(rendered.contains("price=_datum_text(row.value_by_name(Items.PRICE)),"));
}

#[test]
fn renders_pk_helpers() {
    let template = TemplateEntityPy::from_table_schema(&sample_table_def()).unwrap();
    let rendered = template.render().unwrap();

    assert!(rendered.contains("def get_by_pk("));
    assert!(rendered.contains("item_id: int,"));
    assert!(rendered.contains(") -> Optional[Items]:"));
    assert!(rendered.contains("params.bind(0, item_id)"));
    assert!(rendered.contains("def delete_by_pk("));
}

#[test]
fn omits_pk_helpers_when_table_has_no_primary_key() {
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
    let template = TemplateEntityPy::from_table_schema(&table).unwrap();
    let rendered = template.render().unwrap();

    assert!(!rendered.contains("SQL_GET_BY_PK"));
    assert!(!rendered.contains("SQL_DELETE_BY_PK"));
    assert!(!rendered.contains("def get_by_pk("));
    assert!(!rendered.contains("Optional"));
    assert!(rendered.contains("SQL_INSERT: ClassVar[str] ="));
    assert!(rendered.contains("from mududb.result import Row, as_string"));
}

#[test]
fn rejects_columns_the_facade_cannot_transport() {
    for (name, ty) in [
        ("huge", UniDataType::Scalar(UniScalar::I128)),
        ("named", UniDataType::Identifier("photo_fs".to_string())),
    ] {
        let table = TableDef::new(
            "t".to_string(),
            vec![ColumnDef::new(name.to_string(), ty, None, true, false)],
        );
        let err = TemplateEntityPy::from_table_schema(&table)
            .err()
            .expect("unsupported columns must be rejected");
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    }
}
