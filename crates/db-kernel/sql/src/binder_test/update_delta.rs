#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;

fn counter_binder() -> Binder {
    binder_arc(Arc::new(TestMetaMgr::new(SchemaTable::new(
        "counters".to_string(),
        vec![
            SchemaColumn::new(
                "id".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
            SchemaColumn::new(
                "count".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
        ],
        vec![0],
        vec![1],
    ))))
}

fn money_binder() -> Binder {
    binder_arc(Arc::new(TestMetaMgr::new(SchemaTable::new(
        "accounts".to_string(),
        vec![
            SchemaColumn::new(
                "id".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
            SchemaColumn::new(
                "balance".to_string(),
                TypeFamily::Numeric,
                DataTypeInfo::from_opt_object(&DataType::from_numeric(DataTypeParamNumeric::new(
                    12, 2,
                ))),
            ),
        ],
        vec![0],
        vec![1],
    ))))
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_update_accepts_numeric_column_delta_parameter() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = money_binder()
            .bind(
                parse_stmt("update accounts set balance = balance + ? where id = ?;"),
                &(7i32, 1i32),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Update(update)) = bound else {
            panic!("expected bound update");
        };
        let (op, literal) = expect_delta_assignment(&update);
        assert_eq!(op, DeltaOp::Add);
        // The integer parameter is coerced into the NUMERIC(12,2) column
        // layout so the executor can decode it with the column type.
        let balance_type = DataType::from_numeric(DataTypeParamNumeric::new(12, 2));
        let expected = Numeric::from(7i32).to_binary(&balance_type).unwrap();
        assert_eq!(literal, expected.as_ref());
    })
    .unwrap()
}

fn expect_delta_assignment(update: &crate::bound_stmt::BoundUpdate) -> (DeltaOp, &[u8]) {
    assert_eq!(update.value.len(), 1);
    assert_eq!(update.value[0].0, 1);
    match &update.value[0].1 {
        BoundSetValue::Delta { op, literal } => (*op, literal.as_slice()),
        other => panic!("expected delta assignment, got {other:?}"),
    }
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_update_accepts_self_increment_expression() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = counter_binder()
            .bind(
                parse_stmt("update counters set count = count + 1 where id = 1;"),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Update(update)) = bound else {
            panic!("expected bound update");
        };
        let (op, literal) = expect_delta_assignment(&update);
        assert_eq!(op, DeltaOp::Add);
        let expected = 1i32
            .to_binary(&DataType::default_for(TypeFamily::I32))
            .unwrap();
        assert_eq!(literal, expected.as_ref());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_update_accepts_self_decrement_expression() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = counter_binder()
            .bind(
                parse_stmt("update counters set count = count - 3 where id = 1;"),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Update(update)) = bound else {
            panic!("expected bound update");
        };
        let (op, literal) = expect_delta_assignment(&update);
        assert_eq!(op, DeltaOp::Sub);
        let expected = 3i32
            .to_binary(&DataType::default_for(TypeFamily::I32))
            .unwrap();
        assert_eq!(literal, expected.as_ref());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_update_accepts_self_increment_parameter() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = counter_binder()
            .bind(
                parse_stmt("update counters set count = count + ? where id = ?;"),
                &(5i32, 1i32),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Update(update)) = bound else {
            panic!("expected bound update");
        };
        let (op, literal) = expect_delta_assignment(&update);
        assert_eq!(op, DeltaOp::Add);
        let expected = 5i32
            .to_binary(&DataType::default_for(TypeFamily::I32))
            .unwrap();
        assert_eq!(literal, expected.as_ref());
        // The delta parameter consumed index 0, so the key predicate must
        // bind parameter index 1.
        assert_eq!(update.key.len(), 1);
        let expected_key = 1i32
            .to_binary(&DataType::default_for(TypeFamily::I32))
            .unwrap();
        assert_eq!(update.key[0].1, expected_key.as_ref());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_update_accepts_self_decrement_parameter() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = counter_binder()
            .bind(
                parse_stmt("update counters set count = count - ? where id = ?;"),
                &(3i32, 1i32),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Update(update)) = bound else {
            panic!("expected bound update");
        };
        let (op, literal) = expect_delta_assignment(&update);
        assert_eq!(op, DeltaOp::Sub);
        let expected = 3i32
            .to_binary(&DataType::default_for(TypeFamily::I32))
            .unwrap();
        assert_eq!(literal, expected.as_ref());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_update_rejects_non_integer_delta_parameter() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = counter_binder()
            .bind(
                parse_stmt("update counters set count = count + ? where id = ?;"),
                &(1.5f64, 1i32),
            )
            .await
            .unwrap_err();

        assert!(err
            .to_string()
            .contains("expression updates are not implemented"));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_update_rejects_unsupported_expression_forms() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        // Left operand must be the assigned column itself.
        for sql in [
            // Different column on the left.
            "update counters set count = id + 1 where id = 1;",
            // Literal on the left.
            "update counters set count = 1 + count where id = 1;",
            // Only + and - are supported.
            "update counters set count = count * 2 where id = 1;",
            // Placeholder on the left is not the assigned column.
            "update counters set count = ? + count where id = 1;",
            // Non-integer assigned column.
            "update users set name = name + 1 where id = 1;",
            // Non-integer literal.
            "update counters set count = count + 'x' where id = 1;",
        ] {
            let b = if sql.contains("users") {
                binder()
            } else {
                counter_binder()
            };
            let err = b.bind(parse_stmt(sql), &()).await.unwrap_err();
            assert!(
                err.to_string()
                    .contains("expression updates are not implemented"),
                "expected NotImplemented for {sql}, got {err}"
            );
        }
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_update_accepts_string_encoded_numeric_delta_placeholder() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        // NUMERIC params arrive type-erased as strings over the wire; the
        // binder must accept them for delta assignments on numeric columns.
        let bal_type = DataType::from_numeric(DataTypeParamNumeric::new(12, 2));
        let schema = SchemaTable::new(
            "accounts2".to_string(),
            vec![
                SchemaColumn::new(
                    "id".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
                SchemaColumn::new(
                    "bal".to_string(),
                    TypeFamily::Numeric,
                    DataTypeInfo::from_opt_object(&bal_type),
                ),
            ],
            vec![0],
            vec![1],
        );
        let binder = binder_arc(Arc::new(TestMetaMgr::new(schema)));
        let params = mudu_contract::database::sql_param_value::SQLParamValue::from_vec(vec![
            DataValue::from_string("3.00".to_string()),
            DataValue::from_i32(1),
        ]);
        let bound = binder
            .bind(
                parse_stmt("update accounts2 set bal = bal - ? where id = ?;"),
                &params,
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Update(update)) = bound else {
            panic!("expected bound update");
        };
        match &update.value[0].1 {
            BoundSetValue::Delta { op, literal } => {
                assert_eq!(*op, DeltaOp::Sub);
                let expected = DataValue::from_numeric(Numeric::parse("3.00").unwrap())
                    .to_binary(&bal_type)
                    .unwrap();
                assert_eq!(literal.as_slice(), expected.as_ref());
            }
            other => panic!("expected delta assignment, got {other:?}"),
        }
    })
    .unwrap()
}
