use crate::value_update::{
    apply_value_update, apply_value_update_with_deltas, encode_sub_wrap_literal,
};
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::table_info::TableInfo;
use ::xengine::api::{DeltaAssign, DeltaOp, VecDatum};
use mudu::common::id::AttrIndex;
use mudu::data_type::numeric::Numeric;
use mudu_contract::tuple::nullable_tuple::{read_value, NullableValue, TupleBuilder};
use mudu_type::data_type_function::send_binary;
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::data_value::DataValue;
use mudu_type::type_family::TypeFamily;
use std::sync::Arc;

const ATTR_A: AttrIndex = 1; // i32
const ATTR_B: AttrIndex = 2; // i64
const ATTR_C: AttrIndex = 3; // i128
const ATTR_D: AttrIndex = 4; // u128
const ATTR_E: AttrIndex = 5; // numeric(18,2)
const ATTR_F: AttrIndex = 6; // varchar(64)

fn test_desc() -> Arc<TableDesc> {
    let col = |name: &str, family: TypeFamily, param: &str| {
        SchemaColumn::new(
            name.to_string(),
            family,
            DataTypeInfo::from_text(family, param.to_string()),
        )
    };
    TableInfo::new(SchemaTable::new(
        "t".to_string(),
        vec![
            col("id", TypeFamily::I32, ""),
            col("a", TypeFamily::I32, ""),
            col("b", TypeFamily::I64, ""),
            col("c", TypeFamily::I128, ""),
            col("d", TypeFamily::U128, ""),
            col("e", TypeFamily::Numeric, r#"{"precision":18,"scale":2}"#),
            col("f", TypeFamily::String, r#"{"length":64}"#),
        ],
        vec![0],
        vec![1, 2, 3, 4, 5, 6],
    ))
    .unwrap()
    .table_desc()
    .unwrap()
}

fn enc(desc: &TableDesc, attr: AttrIndex, value: &DataValue) -> Vec<u8> {
    send_binary(value, desc.get_attr(attr).type_desc()).unwrap()
}

/// Builds the current value tuple from (attribute, value) pairs; the tuple
/// layout is by datum index, which is not the column order.
fn current_tuple(desc: &TableDesc, by_attr: Vec<(AttrIndex, NullableValue)>) -> Vec<u8> {
    let mut slots: Vec<Option<NullableValue>> = vec![None; desc.value_desc().field_count()];
    for (attr, value) in by_attr {
        let datum_index = desc.get_attr(attr).datum_index();
        assert!(
            slots[datum_index].is_none(),
            "duplicate datum {datum_index}"
        );
        slots[datum_index] = Some(value);
    }
    let slots: Vec<NullableValue> = slots.into_iter().map(|s| s.unwrap()).collect();
    TupleBuilder::new(desc.value_desc()).build(&slots).unwrap()
}

fn base_values() -> Vec<(AttrIndex, NullableValue)> {
    vec![
        (ATTR_A, NullableValue::Value(DataValue::from_i32(1))),
        (ATTR_B, NullableValue::Value(DataValue::from_i64(100))),
        (ATTR_C, NullableValue::Value(DataValue::from_i128(1000))),
        (ATTR_D, NullableValue::Value(DataValue::from_u128(10000))),
        (
            ATTR_E,
            NullableValue::Value(DataValue::from_numeric(Numeric::parse("100.00").unwrap())),
        ),
        (
            ATTR_F,
            NullableValue::Value(DataValue::from_string("hello".to_string())),
        ),
    ]
}

fn set_value(values: &mut [(AttrIndex, NullableValue)], attr: AttrIndex, value: DataValue) {
    values.iter_mut().find(|(a, _)| *a == attr).unwrap().1 = NullableValue::Value(value);
}

fn read_field(tuple: &Vec<u8>, desc: &TableDesc, attr: AttrIndex) -> NullableValue {
    read_value(tuple, desc.value_desc(), desc.get_attr(attr).datum_index()).unwrap()
}

fn read_data_value(tuple: &Vec<u8>, desc: &TableDesc, attr: AttrIndex) -> DataValue {
    match read_field(tuple, desc, attr) {
        NullableValue::Value(value) => value,
        NullableValue::Null => panic!("expected a non-NULL value"),
    }
}

fn delta(attr: AttrIndex, op: DeltaOp, literal: Vec<u8>) -> DeltaAssign {
    DeltaAssign { attr, op, literal }
}

fn apply_deltas(desc: &TableDesc, current: &Vec<u8>, deltas: Vec<DeltaAssign>) -> Vec<u8> {
    apply_value_update_with_deltas(current, &VecDatum::new(vec![]), &deltas, &[], desc).unwrap()
}

#[test]
fn delta_add_sub_across_integer_types() {
    let desc = test_desc();
    let current = current_tuple(&desc, base_values());
    let updated = apply_deltas(
        &desc,
        &current,
        vec![
            delta(
                ATTR_B,
                DeltaOp::Add,
                enc(&desc, ATTR_B, &DataValue::from_i64(23)),
            ),
            delta(
                ATTR_C,
                DeltaOp::Sub,
                enc(&desc, ATTR_C, &DataValue::from_i128(1)),
            ),
            delta(
                ATTR_D,
                DeltaOp::Add,
                enc(&desc, ATTR_D, &DataValue::from_u128(5)),
            ),
        ],
    );
    assert_eq!(*read_data_value(&updated, &desc, ATTR_B).expect_i64(), 123);
    assert_eq!(*read_data_value(&updated, &desc, ATTR_C).expect_i128(), 999);
    assert_eq!(
        *read_data_value(&updated, &desc, ATTR_D).expect_u128(),
        10005
    );
    // Untouched columns survive the update.
    assert_eq!(*read_data_value(&updated, &desc, ATTR_A).expect_i32(), 1);
}

#[test]
fn delta_numeric_add_and_sub() {
    let desc = test_desc();
    let current = current_tuple(&desc, base_values());
    let subbed = apply_deltas(
        &desc,
        &current,
        vec![delta(
            ATTR_E,
            DeltaOp::Sub,
            enc(
                &desc,
                ATTR_E,
                &DataValue::from_numeric(Numeric::parse("1.50").unwrap()),
            ),
        )],
    );
    assert_eq!(
        *read_data_value(&subbed, &desc, ATTR_E).expect_numeric(),
        Numeric::parse("98.50").unwrap()
    );
    let added = apply_deltas(
        &desc,
        &subbed,
        vec![delta(
            ATTR_E,
            DeltaOp::Add,
            enc(
                &desc,
                ATTR_E,
                &DataValue::from_numeric(Numeric::parse("0.25").unwrap()),
            ),
        )],
    );
    assert_eq!(
        *read_data_value(&added, &desc, ATTR_E).expect_numeric(),
        Numeric::parse("98.75").unwrap()
    );
}

#[test]
fn delta_integer_overflow_is_reported() {
    let desc = test_desc();
    let mut values = base_values();
    set_value(&mut values, ATTR_A, DataValue::from_i32(i32::MAX));
    let current = current_tuple(&desc, values);
    let err = apply_value_update_with_deltas(
        &current,
        &VecDatum::new(vec![]),
        &[delta(
            ATTR_A,
            DeltaOp::Add,
            enc(&desc, ATTR_A, &DataValue::from_i32(1)),
        )],
        &[],
        &desc,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("overflows the column type"),
        "unexpected: {err}"
    );
}

#[test]
fn delta_requires_integer_column() {
    let desc = test_desc();
    let current = current_tuple(&desc, base_values());
    let err = apply_value_update_with_deltas(
        &current,
        &VecDatum::new(vec![]),
        &[delta(
            ATTR_F,
            DeltaOp::Add,
            enc(&desc, ATTR_F, &DataValue::from_string("x".to_string())),
        )],
        &[],
        &desc,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("requires an integer column"),
        "unexpected: {err}"
    );
}

#[test]
fn delta_over_null_column_becomes_null_assignment() {
    let desc = test_desc();
    let mut values = base_values();
    values.iter_mut().find(|(a, _)| *a == ATTR_A).unwrap().1 = NullableValue::Null;
    let current = current_tuple(&desc, values);
    let updated = apply_deltas(
        &desc,
        &current,
        vec![delta(
            ATTR_A,
            DeltaOp::Add,
            enc(&desc, ATTR_A, &DataValue::from_i32(1)),
        )],
    );
    assert!(matches!(
        read_field(&updated, &desc, ATTR_A),
        NullableValue::Null
    ));
}

#[test]
fn null_assignment_rebuilds_and_preserves() {
    let desc = test_desc();
    let current = current_tuple(&desc, base_values());
    let updated = apply_value_update(&current, &VecDatum::new(vec![]), &[ATTR_B], &desc).unwrap();
    assert!(matches!(
        read_field(&updated, &desc, ATTR_B),
        NullableValue::Null
    ));
    assert_eq!(*read_data_value(&updated, &desc, ATTR_A).expect_i32(), 1);
    assert_eq!(
        *read_data_value(&updated, &desc, ATTR_C).expect_i128(),
        1000
    );
}

#[test]
fn update_of_currently_null_column_rebuilds() {
    let desc = test_desc();
    let mut values = base_values();
    values.iter_mut().find(|(a, _)| *a == ATTR_A).unwrap().1 = NullableValue::Null;
    let current = current_tuple(&desc, values);
    let updated = apply_value_update(
        &current,
        &VecDatum::new(vec![(ATTR_A, enc(&desc, ATTR_A, &DataValue::from_i32(5)))]),
        &[],
        &desc,
    )
    .unwrap();
    assert_eq!(*read_data_value(&updated, &desc, ATTR_A).expect_i32(), 5);
}

#[test]
fn rebuild_rejects_key_duplicate_and_undecodable_assignments() {
    let desc = test_desc();
    let current = current_tuple(&desc, base_values());

    // Primary-key columns cannot be assigned.
    let err = apply_value_update(
        &current,
        &VecDatum::new(vec![(0, 1i32.to_be_bytes().to_vec())]),
        &[ATTR_B],
        &desc,
    )
    .unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidTuple);

    // The same attribute twice is a duplicate assignment.
    let err = apply_value_update(
        &current,
        &VecDatum::new(vec![
            (ATTR_B, enc(&desc, ATTR_B, &DataValue::from_i64(1))),
            (ATTR_B, enc(&desc, ATTR_B, &DataValue::from_i64(2))),
        ]),
        &[ATTR_A],
        &desc,
    )
    .unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidTuple);

    // A datum that does not decode as the column type is rejected.
    assert!(apply_value_update(
        &current,
        &VecDatum::new(vec![(ATTR_B, vec![0xAB])]),
        &[ATTR_A],
        &desc,
    )
    .is_err());
}

#[test]
fn restock_rejects_bad_literal_length() {
    let desc = test_desc();
    let current = current_tuple(&desc, base_values());
    let err = apply_value_update_with_deltas(
        &current,
        &VecDatum::new(vec![]),
        &[delta(ATTR_B, DeltaOp::SubWrapDeferred, vec![0; 8])],
        &[],
        &desc,
    )
    .unwrap_err();
    assert!(err.to_string().contains("24-byte"), "unexpected: {err}");
}

#[test]
fn restock_requires_wrap_above_floor() {
    let desc = test_desc();
    let current = current_tuple(&desc, base_values());
    let err = apply_value_update_with_deltas(
        &current,
        &VecDatum::new(vec![]),
        &[delta(
            ATTR_B,
            DeltaOp::SubWrapDeferred,
            encode_sub_wrap_literal(3, 100, 10),
        )],
        &[],
        &desc,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("wrap > floor"),
        "unexpected: {err}"
    );
}

#[test]
fn restock_wraps_below_floor_and_keeps_above() {
    let desc = test_desc();
    let current = current_tuple(&desc, base_values());
    // b = 100: y = 100 - 3 = 97 >= floor 10 -> stays 97.
    let updated = apply_deltas(
        &desc,
        &current,
        vec![delta(
            ATTR_B,
            DeltaOp::SubWrapDeferred,
            encode_sub_wrap_literal(3, 10, 100),
        )],
    );
    assert_eq!(*read_data_value(&updated, &desc, ATTR_B).expect_i64(), 97);

    // a = 1: y = 1 - 5 = -4 < floor 10 -> wraps to -4 + 100 = 96.
    let updated = apply_deltas(
        &desc,
        &current,
        vec![delta(
            ATTR_A,
            DeltaOp::SubWrapDeferred,
            encode_sub_wrap_literal(5, 10, 100),
        )],
    );
    assert_eq!(*read_data_value(&updated, &desc, ATTR_A).expect_i32(), 96);
}

#[test]
fn restock_i32_result_overflow_is_reported() {
    let desc = test_desc();
    let mut values = base_values();
    set_value(&mut values, ATTR_A, DataValue::from_i32(i32::MAX));
    let current = current_tuple(&desc, values);
    // y = MAX - (-1) = MAX + 1 stays in range (floor 0), but no longer fits i32.
    let err = apply_value_update_with_deltas(
        &current,
        &VecDatum::new(vec![]),
        &[delta(
            ATTR_A,
            DeltaOp::SubWrapDeferred,
            encode_sub_wrap_literal(-1, 0, 10),
        )],
        &[],
        &desc,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("overflows i32"),
        "unexpected: {err}"
    );
}

#[test]
fn restock_requires_i32_or_i64_column() {
    let desc = test_desc();
    let current = current_tuple(&desc, base_values());
    let err = apply_value_update_with_deltas(
        &current,
        &VecDatum::new(vec![]),
        &[delta(
            ATTR_F,
            DeltaOp::SubWrapDeferred,
            encode_sub_wrap_literal(1, 0, 10),
        )],
        &[],
        &desc,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("requires an i32/i64 column"),
        "unexpected: {err}"
    );
}

#[test]
fn restock_arithmetic_overflow_is_reported() {
    let desc = test_desc();
    // y = MIN + 10 - 100 overflows i64 subtraction.
    let mut values = base_values();
    set_value(&mut values, ATTR_B, DataValue::from_i64(i64::MIN + 10));
    let current = current_tuple(&desc, values);
    let err = apply_value_update_with_deltas(
        &current,
        &VecDatum::new(vec![]),
        &[delta(
            ATTR_B,
            DeltaOp::SubWrapDeferred,
            encode_sub_wrap_literal(100, 0, 10),
        )],
        &[],
        &desc,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("restock delta overflows"),
        "unexpected: {err}"
    );

    // y = 100 - 95 = 5 < floor 10, but 5 + i64::MAX overflows the wrap.
    let current = current_tuple(&desc, base_values());
    let err = apply_value_update_with_deltas(
        &current,
        &VecDatum::new(vec![]),
        &[delta(
            ATTR_B,
            DeltaOp::SubWrapDeferred,
            encode_sub_wrap_literal(95, 10, i64::MAX),
        )],
        &[],
        &desc,
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("restock wrap overflows"),
        "unexpected: {err}"
    );
}
