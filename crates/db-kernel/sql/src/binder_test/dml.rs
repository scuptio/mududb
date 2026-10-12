#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_builds_key_eq_predicate() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(parse_stmt("select id from users where id = 1;"), &())
            .await
            .unwrap();

        let BoundStmt::Query(BoundQuery::Select(select)) = bound else {
            panic!("expected bound select");
        };
        assert_eq!(select.select_items.len(), 1);
        match &select.select_items[0] {
            BoundSelectItem::Column(column) => assert_eq!(column.attr, 0),
            other => panic!("expected column select item, got {other:?}"),
        }
        match select.predicate {
            BoundPredicate::KeyEq { key } => assert_eq!(key.len(), 1),
            other => panic!("expected key equality predicate, got {other:?}"),
        }
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_table_preserves_nullable_constraints() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(
                parse_stmt(
                    "
                create table accounts (
                    id int primary key,
                    name char(32) not null,
                    nickname char(32)
                );
                ",
                ),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::CreateTable(create)) = bound else {
            panic!("expected create table");
        };
        let columns = create.schema.columns();
        assert!(!columns[0].nullable());
        assert!(!columns[1].nullable());
        assert!(columns[2].nullable());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_uses_key_prefix_eq_for_left_prefix_of_composite_primary_key() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = composite_binder()
            .bind(
                parse_stmt("select tenant_id from accounts where tenant_id = 1;"),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Query(BoundQuery::Select(select)) = bound else {
            panic!("expected bound select");
        };
        match select.predicate {
            BoundPredicate::KeyPrefixEq { prefix } => assert_eq!(prefix.len(), 1),
            other => panic!("expected key prefix equality predicate, got {other:?}"),
        }
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_rejects_non_leftmost_composite_primary_key_equality() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = composite_binder()
            .bind(
                parse_stmt("select tenant_id from accounts where user_id = 2;"),
                &(),
            )
            .await
            .unwrap_err();

        assert!(err
            .to_string()
            .contains("must cover a left prefix of the primary key"));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_reverses_value_column_comparison() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(parse_stmt("select id from users where ? = id;"), &(7i32,))
            .await
            .unwrap();

        let BoundStmt::Query(BoundQuery::Select(select)) = bound else {
            panic!("expected bound select");
        };
        match select.predicate {
            BoundPredicate::KeyEq { key } => assert_eq!(key.len(), 1),
            other => panic!("expected key equality predicate, got {other:?}"),
        }
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_builds_range_predicate_from_placeholder() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(parse_stmt("select id from users where id > ?;"), &(7i32,))
            .await
            .unwrap();

        let BoundStmt::Query(BoundQuery::Select(select)) = bound else {
            panic!("expected bound select");
        };
        match select.predicate {
            BoundPredicate::KeyRange { start, end } => {
                assert!(matches!(start, std::ops::Bound::Excluded(_)));
                assert!(matches!(end, std::ops::Bound::Unbounded));
            }
            other => panic!("expected key range predicate, got {other:?}"),
        }
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_rejects_not_equal_predicate() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(parse_stmt("select id from users where id != 1;"), &())
            .await
            .unwrap_err();

        assert!(err
            .to_string()
            .contains("not-equal predicates are not implemented"));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_rejects_mixed_equality_and_range_predicates() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(
                parse_stmt("select id from users where id = 1 AND id > 0;"),
                &(),
            )
            .await
            .unwrap_err();

        assert!(err
            .to_string()
            .contains("mixed equality and range predicates are not implemented"));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_insert_without_column_list_uses_schema_order() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(parse_stmt("insert into users values (1, 'alice');"), &())
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Insert(insert)) = bound else {
            panic!("expected bound insert");
        };
        assert_eq!(insert.rows.len(), 1);
        assert_eq!(insert.rows[0].key.len(), 1);
        assert_eq!(insert.rows[0].value.len(), 1);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_insert_allows_null_for_nullable_value_column() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(
                parse_stmt("insert into users (id, name) values (1, null);"),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Insert(insert)) = bound else {
            panic!("expected bound insert");
        };
        assert_eq!(insert.rows.len(), 1);
        assert_eq!(insert.rows[0].key.len(), 1);
        assert_eq!(insert.rows[0].value.len(), 0);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_insert_rejects_null_for_primary_key() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(
                parse_stmt("insert into users (id, name) values (null, 'alice');"),
                &(),
            )
            .await
            .unwrap_err();

        assert!(err.to_string().contains("NOT NULL"));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_insert_rejects_null_for_not_null_value_column() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = not_null_value_binder()
            .bind(
                parse_stmt("insert into users (id, name) values (1, null);"),
                &(),
            )
            .await
            .unwrap_err();

        assert!(err.to_string().contains("NOT NULL"));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_insert_accepts_multi_row_insert() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(
                parse_stmt("insert into users (id, name) values (1, 'alice'), (2, 'bob');"),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Insert(insert)) = bound else {
            panic!("expected bound insert");
        };
        assert_eq!(insert.rows.len(), 2);
        assert_eq!(insert.rows[0].key.len(), 1);
        assert_eq!(insert.rows[0].value.len(), 1);
        assert_eq!(insert.rows[1].key.len(), 1);
        assert_eq!(insert.rows[1].value.len(), 1);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_insert_accepts_multi_row_insert_with_placeholders() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(
                parse_stmt("insert into users (id, name) values (?, 'alice'), (?, 'bob');"),
                &(1i32, 2i32),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Insert(insert)) = bound else {
            panic!("expected bound insert");
        };
        assert_eq!(insert.rows.len(), 2);
        assert_eq!(insert.rows[0].key.len(), 1);
        assert_eq!(insert.rows[1].key.len(), 1);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_insert_encodes_numeric_literal_into_declared_column_type() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = numeric_binder()
            .bind(
                parse_stmt("insert into ledger (amount, note) values (12.3400, 'coffee');"),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Insert(insert)) = bound else {
            panic!("expected bound insert");
        };
        let amount_type = DataType::from_numeric(DataTypeParamNumeric::new(9, 2));
        let note_type = DataType::default_for(TypeFamily::String);

        assert_eq!(insert.rows.len(), 1);
        assert_eq!(insert.rows[0].key.len(), 1);
        assert_eq!(insert.rows[0].value.len(), 1);
        assert_eq!(
            insert.rows[0].key[0].1,
            Numeric::parse("12.3400")
                .unwrap()
                .to_binary(&amount_type)
                .unwrap()
                .as_ref()
        );
        assert_eq!(
            insert.rows[0].value[0].1,
            "coffee".to_string().to_binary(&note_type).unwrap().as_ref()
        );
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_insert_string_literal_encodes_without_quotes() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(parse_stmt("insert into users values (1, 'alice');"), &())
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Insert(insert)) = bound else {
            panic!("expected bound insert");
        };
        let name_type = DataType::default_for(TypeFamily::String);
        assert_eq!(
            insert.rows[0].value[0].1,
            "alice".to_string().to_binary(&name_type).unwrap().as_ref()
        );
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_numeric_placeholder_uses_numeric_key_encoding() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = numeric_binder()
            .bind(
                parse_stmt("select amount from ledger where amount = ?;"),
                &(Numeric::parse("12.3400").unwrap(),),
            )
            .await
            .unwrap();

        let BoundStmt::Query(BoundQuery::Select(select)) = bound else {
            panic!("expected bound select");
        };
        let amount_type = DataType::from_numeric(DataTypeParamNumeric::new(9, 2));
        let expected = Numeric::parse("12.3400")
            .unwrap()
            .to_binary(&amount_type)
            .unwrap();
        match select.predicate {
            BoundPredicate::KeyEq { key } => {
                assert_eq!(key.len(), 1);
                assert_eq!(key[0].1, expected.as_ref());
            }
            other => panic!("expected key equality predicate, got {other:?}"),
        }
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_insert_rejects_column_size_mismatch() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(
                parse_stmt("insert into users (id) values (1, 'alice');"),
                &(),
            )
            .await
            .unwrap_err();

        assert!(err.to_string().contains("insert column size mismatch"));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_update_rejects_primary_key_updates() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(parse_stmt("update users set id = 2 where id = 1;"), &())
            .await
            .unwrap_err();

        assert!(err
            .to_string()
            .contains("updating primary key columns is not implemented"));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_update_rejects_expression_updates() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(
                parse_stmt("update users set name = id + 1 where id = 1;"),
                &(),
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
fn bind_update_allows_null_for_nullable_value_column() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(
                parse_stmt("update users set name = null where id = 1;"),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Update(update)) = bound else {
            panic!("expected bound update");
        };
        assert_eq!(update.value.len(), 1);
        assert_eq!(update.value[0].0, 1);
        assert!(
            matches!(&update.value[0].1, BoundSetValue::Null),
            "expected NULL assignment, got {:?}",
            update.value[0].1
        );
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_delete_rejects_non_key_predicates() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(parse_stmt("delete from users where name = 'alice';"), &())
            .await
            .unwrap_err();

        assert!(err
            .to_string()
            .contains("non-key predicates are not implemented"));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_delete_requires_complete_composite_primary_key() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = composite_binder()
            .bind(parse_stmt("delete from accounts where tenant_id = 1;"), &())
            .await
            .unwrap_err();

        assert!(err.to_string().contains("complete primary key predicate"));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_delete_accepts_complete_composite_primary_key() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = composite_binder()
            .bind(
                parse_stmt("delete from accounts where tenant_id = 1 AND user_id = 2;"),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Delete(delete)) = bound else {
            panic!("expected bound delete");
        };
        assert_eq!(delete.key.len(), 2);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_builds_range_predicates_for_ge_le_lt_gt() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        for (sql, expected_start, expected_end) in [
            (
                "select id from users where id >= 1;",
                "Included",
                "Unbounded",
            ),
            (
                "select id from users where id > 1;",
                "Excluded",
                "Unbounded",
            ),
            (
                "select id from users where id <= 1;",
                "Unbounded",
                "Included",
            ),
            (
                "select id from users where id < 1;",
                "Unbounded",
                "Excluded",
            ),
        ] {
            let bound = binder().bind(parse_stmt(sql), &()).await.unwrap();
            let BoundStmt::Query(BoundQuery::Select(select)) = bound else {
                panic!("expected bound select for {sql}");
            };
            match select.predicate {
                BoundPredicate::KeyRange { start, end } => {
                    assert!(format!("{start:?}").starts_with(expected_start));
                    assert!(format!("{end:?}").starts_with(expected_end));
                }
                other => panic!("expected key range for {sql}, got {other:?}"),
            }
        }
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_update_accepts_complete_primary_key() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(
                parse_stmt("update users set name = 'alice' where id = 1;"),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Update(update)) = bound else {
            panic!("expected bound update");
        };
        assert_eq!(update.key.len(), 1);
        assert_eq!(update.value.len(), 1);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_update_rejects_null_for_not_null_value_column() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = not_null_value_binder()
            .bind(
                parse_stmt("update users set name = null where id = 1;"),
                &(),
            )
            .await
            .unwrap_err();

        assert!(err.to_string().contains("NOT NULL"));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_insert_with_explicit_column_list_preserves_order() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(
                parse_stmt("insert into users (name, id) values ('alice', 1);"),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Insert(insert)) = bound else {
            panic!("expected bound insert");
        };
        assert_eq!(insert.rows.len(), 1);
        assert_eq!(insert.rows[0].key[0].0, 0);
        assert_eq!(insert.rows[0].value[0].0, 1);
    })
    .unwrap()
}
