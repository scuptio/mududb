#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use super::*;

#[test]
#[cfg_attr(miri, ignore)]
fn test_create_table() {
    let sql = "
CREATE TABLE Persons (
    PersonID int PRIMARY KEY,
    LastName char(255),
    FirstName char(255),
    Address char(255),
    City char(255)
);";
    let r = parse_sql(sql);
    assert!(r.is_ok());

    let sql = "
CREATE TABLE CUSTOMERS(
       ID1          INT,
       ID2          INT,
       NAME        CHAR (20),
       AGE         INT,
       ADDRESS     CHAR (25),
       SALARY      INT,
       PRIMARY KEY (ID1, ID2)
);";
    let r = parse_sql(sql);
    assert!(r.is_ok());
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_table_with_multibyte_comment_characters() {
    let stmt = parse_create_table(
        "-- leading comment with em-dash — and CJK 汉字
        CREATE TABLE users (
            id INT PRIMARY KEY,
            -- comment between columns — 汉字
            name VARCHAR(100) NOT NULL
        );",
    )
    .unwrap();

    let name = stmt
        .non_primary_columns()
        .into_iter()
        .find(|column| column.column_name() == "name")
        .expect("name column");
    let params = name
        .data_type_param()
        .as_ref()
        .expect("varchar params should exist");
    assert_eq!(params.len(), 1);
    assert!(matches!(
        params[0],
        UniDataValue::Scalar(UniScalarValue::I64(100))
    ));
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_table_after_multibyte_block_comment() {
    let stmt = parse_create_table(
        "/* block comment with em-dash — and CJK 汉字 */
        CREATE TABLE users (
            id INT PRIMARY KEY,
            name VARCHAR(100)
        );",
    )
    .unwrap();

    let name = stmt
        .non_primary_columns()
        .into_iter()
        .find(|column| column.column_name() == "name")
        .expect("name column");
    let params = name
        .data_type_param()
        .as_ref()
        .expect("varchar params should exist");
    assert_eq!(params.len(), 1);
    assert!(matches!(
        params[0],
        UniDataValue::Scalar(UniScalarValue::I64(100))
    ));
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_table_tracks_nullable_constraints() {
    let stmt = parse_create_table(
        "
        CREATE TABLE users (
            id INT PRIMARY KEY,
            name CHAR(32) NOT NULL,
            nickname CHAR(32)
        );
        ",
    )
    .unwrap();

    let columns = stmt.column_def();
    assert!(!columns[0].nullable());
    assert!(!columns[1].nullable());
    assert!(columns[2].nullable());
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_table_numeric_type_preserves_precision_and_scale() {
    let stmt = parse_create_table(
        "
        CREATE TABLE ledger (
            id BIGINT PRIMARY KEY,
            amount NUMERIC(18, 2)
        );
        ",
    )
    .unwrap();

    let amount = stmt
        .non_primary_columns()
        .into_iter()
        .find(|column| column.column_name() == "amount")
        .expect("amount column");
    assert!(matches!(
        amount.data_type(),
        UniDataType::Scalar(UniScalar::Numeric)
    ));
    let params = amount
        .data_type_param()
        .as_ref()
        .expect("numeric params should exist");
    assert_eq!(params.len(), 2);
    assert!(matches!(
        params[0],
        UniDataValue::Scalar(UniScalarValue::I64(18))
    ));
    assert!(matches!(
        params[1],
        UniDataValue::Scalar(UniScalarValue::I64(2))
    ));
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_table_decimal_alias_maps_to_numeric_scalar() {
    let stmt = parse_create_table(
        "
        CREATE TABLE ledger (
            id BIGINT PRIMARY KEY,
            amount DECIMAL(9, 4)
        );
        ",
    )
    .unwrap();

    let amount = stmt
        .non_primary_columns()
        .into_iter()
        .find(|column| column.column_name() == "amount")
        .expect("amount column");
    assert!(matches!(
        amount.data_type(),
        UniDataType::Scalar(UniScalar::Numeric)
    ));
    let params = amount.data_type_param().as_ref().expect("decimal params");
    assert_eq!(params.len(), 2);
    assert!(matches!(
        params[0],
        UniDataValue::Scalar(UniScalarValue::I64(9))
    ));
    assert!(matches!(
        params[1],
        UniDataValue::Scalar(UniScalarValue::I64(4))
    ));
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_table_numeric_without_params_keeps_numeric_type_without_param_payload() {
    let stmt = parse_create_table(
        "
        CREATE TABLE ledger (
            id BIGINT PRIMARY KEY,
            amount NUMERIC
        );
        ",
    )
    .unwrap();

    let amount = stmt
        .non_primary_columns()
        .into_iter()
        .find(|column| column.column_name() == "amount")
        .expect("amount column");
    assert!(matches!(
        amount.data_type(),
        UniDataType::Scalar(UniScalar::Numeric)
    ));
    assert!(amount.data_type_param().is_none());
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_table_temporal_types_preserve_scalar_kinds_and_precision() {
    let stmt = parse_create_table(
        "
        CREATE TABLE events (
            id BIGINT PRIMARY KEY,
            event_date DATE,
            event_time TIME(3),
            created_at TIMESTAMP(4),
            published_at TIMESTAMPTZ(2)
        );
        ",
    )
    .unwrap();

    let columns = stmt.non_primary_columns();
    assert!(matches!(
        columns[0].data_type(),
        UniDataType::Scalar(UniScalar::Date)
    ));
    assert!(columns[0].data_type_param().is_none());

    assert!(matches!(
        columns[1].data_type(),
        UniDataType::Scalar(UniScalar::Time)
    ));
    let time_params = columns[1]
        .data_type_param()
        .as_ref()
        .expect("time params should exist");
    assert_eq!(time_params.len(), 1);
    assert!(matches!(
        time_params[0],
        UniDataValue::Scalar(UniScalarValue::I64(3))
    ));

    assert!(matches!(
        columns[2].data_type(),
        UniDataType::Scalar(UniScalar::Timestamp)
    ));
    let timestamp_params = columns[2]
        .data_type_param()
        .as_ref()
        .expect("timestamp params should exist");
    assert_eq!(timestamp_params.len(), 1);
    assert!(matches!(
        timestamp_params[0],
        UniDataValue::Scalar(UniScalarValue::I64(4))
    ));

    assert!(matches!(
        columns[3].data_type(),
        UniDataType::Scalar(UniScalar::TimestampTz)
    ));
    let timestamptz_params = columns[3]
        .data_type_param()
        .as_ref()
        .expect("timestamptz params should exist");
    assert_eq!(timestamptz_params.len(), 1);
    assert!(matches!(
        timestamptz_params[0],
        UniDataValue::Scalar(UniScalarValue::I64(2))
    ));
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_table_temporal_types_without_precision_keep_no_param_payload() {
    let stmt = parse_create_table(
        "
        CREATE TABLE audit_log (
            id BIGINT PRIMARY KEY,
            event_time TIME,
            created_at TIMESTAMP,
            published_at TIMESTAMPTZ
        );
        ",
    )
    .unwrap();

    let columns = stmt.non_primary_columns();
    assert!(columns[0].data_type_param().is_none());
    assert!(columns[1].data_type_param().is_none());
    assert!(columns[2].data_type_param().is_none());
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_table_hugeint_maps_to_i128() {
    let stmt = parse_create_table(
        "
        CREATE TABLE ledger (
            id BIGINT PRIMARY KEY,
            amount HUGEINT
        );
        ",
    )
    .unwrap();

    let amount = stmt
        .non_primary_columns()
        .into_iter()
        .find(|column| column.column_name() == "amount")
        .expect("amount column");
    assert!(matches!(
        amount.data_type(),
        UniDataType::Scalar(UniScalar::I128)
    ));
    assert!(amount.data_type_param().is_none());
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_table_unknown_identifier_type_maps_to_named_type() {
    let stmt = parse_create_table(
        "
        CREATE TABLE product (
            id INT PRIMARY KEY,
            photo photo_fs
        );
        ",
    )
    .unwrap();

    let id = stmt
        .primary_columns()
        .into_iter()
        .find(|column| column.column_name() == "id")
        .expect("id column");
    assert!(matches!(
        id.data_type(),
        UniDataType::Scalar(UniScalar::I32)
    ));

    let photo = stmt
        .non_primary_columns()
        .into_iter()
        .find(|column| column.column_name() == "photo")
        .expect("photo column");
    assert_eq!(
        photo.data_type().as_identifier(),
        Some(&"photo_fs".to_string())
    );
    assert!(photo.data_type_param().is_none());
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_table_unknown_identifier_type_preserves_case() {
    let stmt = parse_create_table(
        "
        CREATE TABLE product (
            id INT PRIMARY KEY,
            photo Photo_FS2
        );
        ",
    )
    .unwrap();

    let photo = stmt
        .non_primary_columns()
        .into_iter()
        .find(|column| column.column_name() == "photo")
        .expect("photo column");
    assert_eq!(
        photo.data_type().as_identifier(),
        Some(&"Photo_FS2".to_string())
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn parse_create_table_unknown_keyword_type_still_returns_not_implemented() {
    let err = parse_sql("create table users (id boolean primary key);").unwrap_err();
    assert!(err.to_string().contains("not yet implemented"));
}
