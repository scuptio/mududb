// Miri cannot execute FFI calls into the tree-sitter C parser, which is
// used by SQLParser inside this module. Individual tests are skipped under
// Miri; binder behavior is still exercised by normal `cargo test`.
#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use crate::contract::fs_type::{FsColumnBinding, FsTypeDesc, FsTypeKind};
    use crate::contract::meta_mgr::MetaMgr;
    use crate::contract::partition_rule::{PartitionBound, PartitionRuleDesc, RangePartitionDef};
    use crate::contract::partition_rule_binding::PartitionPlacement;
    use crate::contract::schema_column::SchemaColumn;
    use crate::contract::schema_table::SchemaTable;
    use crate::contract::table_desc::TableDesc;
    use crate::contract::table_info::TableInfo;
    use crate::sql::binder::Binder;
    use crate::sql::bound_stmt::{
        AggregateFunc, BoundCommand, BoundPredicate, BoundQuery, BoundSelectItem, BoundSetValue,
        BoundStmt,
    };
    use crate::x_engine::api::DeltaOp;
    use async_trait::async_trait;
    use mudu::common::id::OID;
    use mudu::common::result::RS;
    use mudu::data_type::numeric::Numeric;
    use mudu::error::ErrorCode;
    use mudu::mudu_error;
    use mudu_sys::sync::SMutex;
    use mudu_type::data_type::DataType;
    use mudu_type::data_type_info::DataTypeInfo;
    use mudu_type::data_type_param_numeric::DataTypeParamNumeric;
    use mudu_type::data_value::DataValue;
    use mudu_type::datum::DatumDyn;
    use mudu_type::type_family::TypeFamily;
    use sql_parser::ast::expr_operator::ValueCompare;
    use sql_parser::ast::parser::SQLParser;
    use sql_parser::ast::stmt_type::StmtType;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct TestMetaMgr {
        tables: SMutex<HashMap<OID, Arc<TableDesc>>>,
        rules: SMutex<HashMap<String, PartitionRuleDesc>>,
        fs_types: SMutex<HashMap<String, FsTypeDesc>>,
    }

    impl TestMetaMgr {
        fn new(schema: SchemaTable) -> Self {
            let table = TableInfo::new(schema).unwrap().table_desc().unwrap();
            let mut tables = HashMap::new();
            tables.insert(table.id(), table);
            Self {
                tables: SMutex::new(tables),
                rules: SMutex::new(HashMap::new()),
                fs_types: SMutex::new(HashMap::new()),
            }
        }

        fn with_rule(schema: SchemaTable, rule: PartitionRuleDesc) -> Self {
            let mgr = Self::new(schema);
            mgr.rules.lock().unwrap().insert(rule.name.clone(), rule);
            mgr
        }

        fn with_fs_type(schema: SchemaTable, fs_type: FsTypeDesc) -> Self {
            let mgr = Self::new(schema);
            mgr.fs_types
                .lock()
                .unwrap()
                .insert(fs_type.name().to_string(), fs_type);
            mgr
        }
    }

    #[async_trait]
    impl MetaMgr for TestMetaMgr {
        async fn initialize(&self) -> RS<()> {
            Ok(())
        }

        async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
            self.tables
                .lock()
                .unwrap()
                .get(&oid)
                .cloned()
                .ok_or_else(|| {
                    mudu_error!(ErrorCode::EntityNotFound, format!("no such table {}", oid))
                })
        }

        async fn get_table_by_name(&self, name: &str) -> RS<Option<Arc<TableDesc>>> {
            Ok(self
                .tables
                .lock()
                .unwrap()
                .values()
                .find(|table| table.name() == name)
                .cloned())
        }

        async fn create_table(&self, schema: &SchemaTable) -> RS<()> {
            let table = TableInfo::new(schema.clone())?.table_desc()?;
            self.tables.lock().unwrap().insert(table.id(), table);
            Ok(())
        }

        async fn drop_table(&self, table_id: OID) -> RS<()> {
            self.tables.lock().unwrap().remove(&table_id);
            Ok(())
        }

        async fn create_partition_rule(&self, rule: &PartitionRuleDesc) -> RS<()> {
            self.rules
                .lock()
                .unwrap()
                .insert(rule.name.clone(), rule.clone());
            Ok(())
        }

        async fn get_partition_rule_by_name(&self, name: &str) -> RS<Option<PartitionRuleDesc>> {
            Ok(self.rules.lock().unwrap().get(name).cloned())
        }

        async fn upsert_partition_placements(&self, _placements: &[PartitionPlacement]) -> RS<()> {
            Ok(())
        }

        async fn get_fs_type_by_name(&self, name: &str) -> RS<Option<FsTypeDesc>> {
            Ok(self.fs_types.lock().unwrap().get(name).cloned())
        }
    }

    fn schema() -> SchemaTable {
        SchemaTable::new(
            "users".to_string(),
            vec![
                SchemaColumn::new(
                    "id".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
                SchemaColumn::new(
                    "name".to_string(),
                    TypeFamily::String,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::String)),
                ),
            ],
            vec![0],
            vec![1],
        )
    }

    fn composite_schema() -> SchemaTable {
        SchemaTable::new(
            "accounts".to_string(),
            vec![
                SchemaColumn::new(
                    "tenant_id".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
                SchemaColumn::new(
                    "user_id".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
                SchemaColumn::new(
                    "name".to_string(),
                    TypeFamily::String,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::String)),
                ),
            ],
            vec![0, 1],
            vec![2],
        )
    }

    fn numeric_schema() -> SchemaTable {
        let amount_type = DataType::from_numeric(DataTypeParamNumeric::new(9, 2));
        let note_type = DataType::default_for(TypeFamily::String);
        SchemaTable::new(
            "ledger".to_string(),
            vec![
                SchemaColumn::new(
                    "amount".to_string(),
                    TypeFamily::Numeric,
                    DataTypeInfo::from_opt_object(&amount_type),
                ),
                SchemaColumn::new(
                    "note".to_string(),
                    TypeFamily::String,
                    DataTypeInfo::from_opt_object(&note_type),
                ),
            ],
            vec![0],
            vec![1],
        )
    }

    fn parse_stmt(sql: &str) -> StmtType {
        SQLParser::new().unwrap().parse(sql).unwrap().stmts()[0].clone()
    }

    fn binder() -> Binder {
        Binder::new(Arc::new(TestMetaMgr::new(schema())))
    }

    fn composite_binder() -> Binder {
        Binder::new(Arc::new(TestMetaMgr::new(composite_schema())))
    }

    fn numeric_binder() -> Binder {
        Binder::new(Arc::new(TestMetaMgr::new(numeric_schema())))
    }

    fn not_null_value_binder() -> Binder {
        let id = SchemaColumn::new(
            "id".to_string(),
            TypeFamily::I32,
            DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
        );
        let mut name = SchemaColumn::new(
            "name".to_string(),
            TypeFamily::String,
            DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::String)),
        );
        name.set_nullable(false);
        Binder::new(Arc::new(TestMetaMgr::new(SchemaTable::new(
            "users".to_string(),
            vec![id, name],
            vec![0],
            vec![1],
        ))))
    }

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
                "'coffee'"
                    .to_string()
                    .to_binary(&note_type)
                    .unwrap()
                    .as_ref()
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

    fn counter_binder() -> Binder {
        Binder::new(Arc::new(TestMetaMgr::new(SchemaTable::new(
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
        Binder::new(Arc::new(TestMetaMgr::new(SchemaTable::new(
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
                    DataTypeInfo::from_opt_object(&DataType::from_numeric(
                        DataTypeParamNumeric::new(12, 2),
                    )),
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

    fn expect_delta_assignment(update: &crate::sql::bound_stmt::BoundUpdate) -> (DeltaOp, &[u8]) {
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

    fn rule_with_bounds(name: &str) -> PartitionRuleDesc {
        PartitionRuleDesc::new_range(
            name.to_string(),
            vec![TypeFamily::I32],
            vec![
                RangePartitionDef::new(
                    "p0".to_string(),
                    PartitionBound::Unbounded,
                    PartitionBound::Value(vec![b"100".to_vec()]),
                ),
                RangePartitionDef::new(
                    "p1".to_string(),
                    PartitionBound::Value(vec![b"100".to_vec()]),
                    PartitionBound::Unbounded,
                ),
            ],
        )
    }

    fn partitioned_binder() -> Binder {
        let schema = SchemaTable::new(
            "orders".to_string(),
            vec![
                SchemaColumn::new(
                    "region_id".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
                SchemaColumn::new(
                    "order_id".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
                SchemaColumn::new(
                    "amount".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
                ),
            ],
            vec![0, 1],
            vec![2],
        );
        Binder::new(Arc::new(TestMetaMgr::with_rule(
            schema,
            rule_with_bounds("r_orders"),
        )))
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_create_partition_rule_infers_key_types_from_bounds() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let bound = binder()
                .bind(
                    parse_stmt(
                        "CREATE PARTITION RULE r_test RANGE (
                            PARTITION p0 VALUES FROM (MINVALUE) TO (100),
                            PARTITION p1 VALUES FROM (100) TO (MAXVALUE)
                        );",
                    ),
                    &(),
                )
                .await
                .unwrap();

            let BoundStmt::Command(BoundCommand::CreatePartitionRule(rule)) = bound else {
                panic!("expected create partition rule");
            };
            assert_eq!(rule.rule.key_types, vec![TypeFamily::I64]);
            assert_eq!(rule.rule.partitions.len(), 2);
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_create_partition_placement_resolves_partition_and_worker() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let bound = partitioned_binder()
                .bind(
                    parse_stmt(
                        "CREATE PARTITION PLACEMENT FOR RULE r_orders (
                            PARTITION p0 ON WORKER 11,
                            PARTITION p1 ON WORKER 12
                        );",
                    ),
                    &(),
                )
                .await
                .unwrap();

            let BoundStmt::Command(BoundCommand::CreatePartitionPlacement(placement)) = bound
            else {
                panic!("expected create partition placement");
            };
            assert_eq!(placement.placements.len(), 2);
            assert_eq!(placement.placements[0].worker_id, 11);
            assert_eq!(placement.placements[1].worker_id, 12);
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_create_table_with_partition_binding_resolves_rule() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let bound = partitioned_binder()
                .bind(
                    parse_stmt(
                        "CREATE TABLE orders (
                            region_id INT,
                            order_id INT,
                            amount INT,
                            PRIMARY KEY (region_id, order_id)
                        ) PARTITION BY GLOBAL RULE r_orders REFERENCES (region_id, order_id);",
                    ),
                    &(),
                )
                .await
                .unwrap();

            let BoundStmt::Command(BoundCommand::CreateTable(create)) = bound else {
                panic!("expected create table");
            };
            assert!(create.partition_binding.is_some());
            assert_eq!(
                create.partition_binding.as_ref().unwrap().ref_attr_indices,
                vec![0, 1]
            );
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_create_table_fails_when_partition_rule_not_found() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let err = binder()
                .bind(
                    parse_stmt(
                        "CREATE TABLE missing (
                            id INT PRIMARY KEY
                        ) PARTITION BY GLOBAL RULE no_such_rule REFERENCES (id);",
                    ),
                    &(),
                )
                .await
                .unwrap_err();

            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_create_table_fails_when_partition_reference_column_not_found() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let err = partitioned_binder()
                .bind(
                    parse_stmt(
                        "CREATE TABLE orders (
                            region_id INT,
                            order_id INT,
                            amount INT,
                            PRIMARY KEY (region_id, order_id)
                        ) PARTITION BY GLOBAL RULE r_orders REFERENCES (region_id, missing_col);",
                    ),
                    &(),
                )
                .await
                .unwrap_err();

            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_create_partition_rule_rejects_unbounded_only_rule() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let err = binder()
                .bind(
                    parse_stmt(
                        "CREATE PARTITION RULE r_empty RANGE (
                            PARTITION p0 VALUES FROM (MINVALUE) TO (MAXVALUE)
                        );",
                    ),
                    &(),
                )
                .await
                .unwrap_err();

            assert!(err.to_string().contains("cannot infer partition key types"));
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
    fn bind_update_marks_null_fs_column_assignment_for_rebind() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let mut photo = SchemaColumn::new(
                "photo".to_string(),
                TypeFamily::U128,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::U128)),
            );
            photo.set_fs_binding(Some(FsColumnBinding::new(7, FsTypeKind::File)));
            let schema = SchemaTable::new(
                "product".to_string(),
                vec![
                    SchemaColumn::new(
                        "id".to_string(),
                        TypeFamily::I64,
                        DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I64)),
                    ),
                    photo,
                ],
                vec![0],
                vec![1],
            );
            let bound = Binder::new(Arc::new(TestMetaMgr::new(schema)))
                .bind(
                    parse_stmt("update product set photo = null where id = 1;"),
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
                matches!(&update.value[0].1, BoundSetValue::Absolute(datum) if datum.is_empty()),
                "assigning NULL to an fs column binds the empty-datum rebind sentinel"
            );
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_drop_table_returns_oid_when_table_exists() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let bound = binder()
                .bind(parse_stmt("drop table users;"), &())
                .await
                .unwrap();

            let BoundStmt::Command(BoundCommand::DropTable(drop)) = bound else {
                panic!("expected bound drop table");
            };
            assert!(drop.oid.is_some());
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_drop_table_if_missing_returns_none() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let bound = binder()
                .bind(parse_stmt("drop table if exists missing;"), &())
                .await
                .unwrap();

            let BoundStmt::Command(BoundCommand::DropTable(drop)) = bound else {
                panic!("expected bound drop table");
            };
            assert!(drop.oid.is_none());
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_drop_table_fails_when_missing_without_if_exists() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let err = binder()
                .bind(parse_stmt("drop table missing;"), &())
                .await
                .unwrap_err();

            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_copy_from_builds_layout_for_known_table() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let bound = binder()
                .bind(parse_stmt("copy users from 'users.csv';"), &())
                .await
                .unwrap();

            let BoundStmt::Command(BoundCommand::CopyFrom(copy)) = bound else {
                panic!("expected bound copy from");
            };
            assert_eq!(copy.file_path, "'users.csv'");
            assert_eq!(copy.key_index, vec![0]);
            assert_eq!(copy.value_index, vec![1]);
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_copy_to_builds_layout_for_known_table() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let bound = binder()
                .bind(parse_stmt("copy users to 'users.csv';"), &())
                .await
                .unwrap();

            let BoundStmt::Command(BoundCommand::CopyTo(copy)) = bound else {
                panic!("expected bound copy to");
            };
            assert_eq!(copy.file_path, "'users.csv'");
            assert_eq!(copy.key_indexing, vec![0]);
            assert_eq!(copy.value_indexing, vec![1]);
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_copy_from_fails_for_unknown_table() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let err = binder()
                .bind(parse_stmt("copy missing from 'missing.csv';"), &())
                .await
                .unwrap_err();

            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
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

    fn fs_type_binder(fs_type: FsTypeDesc) -> Binder {
        Binder::new(Arc::new(TestMetaMgr::with_fs_type(schema(), fs_type)))
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_create_table_resolves_registered_fs_type_column_to_u128() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let bound =
                fs_type_binder(FsTypeDesc::new("photo_fs".to_string(), 7, FsTypeKind::File))
                    .bind(
                        parse_stmt("CREATE TABLE product (id INT PRIMARY KEY, photo photo_fs);"),
                        &(),
                    )
                    .await
                    .unwrap();

            let BoundStmt::Command(BoundCommand::CreateTable(create)) = bound else {
                panic!("expected create table");
            };
            let photo = create
                .schema
                .columns()
                .iter()
                .find(|column| column.get_name() == "photo")
                .expect("photo column");
            assert_eq!(photo.type_id(), TypeFamily::U128);
            assert_eq!(
                photo.fs_binding(),
                Some(FsColumnBinding::new(7, FsTypeKind::File))
            );

            let id = create
                .schema
                .columns()
                .iter()
                .find(|column| column.get_name() == "id")
                .expect("id column");
            assert_eq!(id.type_id(), TypeFamily::I32);
            assert_eq!(id.fs_binding(), None);
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_create_table_resolves_directory_fs_type_column() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let bound = fs_type_binder(FsTypeDesc::new(
                "backup_dir".to_string(),
                3,
                FsTypeKind::Directory,
            ))
            .bind(
                parse_stmt("CREATE TABLE product (id INT PRIMARY KEY, backup backup_dir);"),
                &(),
            )
            .await
            .unwrap();

            let BoundStmt::Command(BoundCommand::CreateTable(create)) = bound else {
                panic!("expected create table");
            };
            let backup = create
                .schema
                .columns()
                .iter()
                .find(|column| column.get_name() == "backup")
                .expect("backup column");
            assert_eq!(backup.type_id(), TypeFamily::U128);
            assert_eq!(
                backup.fs_binding(),
                Some(FsColumnBinding::new(3, FsTypeKind::Directory))
            );
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_create_table_fails_for_unregistered_type_name() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let err = binder()
                .bind(
                    parse_stmt("CREATE TABLE product (id INT PRIMARY KEY, photo photo_fs);"),
                    &(),
                )
                .await
                .unwrap_err();

            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
            assert!(err.to_string().contains("unknown type name photo_fs"));
        })
        .unwrap()
    }

    fn bind_select(sql: &'static str) -> crate::sql::bound_stmt::BoundSelect {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let bound = binder().bind(parse_stmt(sql), &()).await.unwrap();
            let BoundStmt::Query(BoundQuery::Select(select)) = bound else {
                panic!("expected bound select");
            };
            select
        })
        .unwrap()
    }

    fn bind_select_err(sql: &'static str) -> ErrorCode {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            binder().bind(parse_stmt(sql), &()).await.unwrap_err().ec()
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_select_aggregate_count_star() {
        let select = bind_select("select count(*) as c from users;");
        assert_eq!(select.select_items.len(), 1);
        let BoundSelectItem::Aggregate(aggregate) = &select.select_items[0] else {
            panic!("expected aggregate select item");
        };
        assert_eq!(aggregate.func, AggregateFunc::Count);
        assert_eq!(aggregate.arg, None);
        assert_eq!(aggregate.result_type.type_family(), TypeFamily::I64);
        assert_eq!(aggregate.output_name, "c");
        assert!(!aggregate.nullable);
        assert_eq!(select.tuple_desc.fields()[0].name(), "c");
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_select_aggregate_result_types() {
        let select = bind_select("select sum(id), avg(id), min(name), max(id) from users;");
        let families: Vec<TypeFamily> = select
            .select_items
            .iter()
            .map(|item| match item {
                BoundSelectItem::Aggregate(aggregate) => aggregate.result_type.type_family(),
                BoundSelectItem::Column(_) => panic!("expected aggregate"),
            })
            .collect();
        assert_eq!(
            families,
            vec![
                TypeFamily::I64,
                TypeFamily::Numeric,
                TypeFamily::String,
                TypeFamily::I32
            ]
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_select_mixed_column_and_aggregate_rejected() {
        assert_eq!(
            bind_select_err("select id, count(*) from users;"),
            ErrorCode::NotImplemented
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_select_unsupported_function_rejected() {
        assert_eq!(
            bind_select_err("select foo(id) from users;"),
            ErrorCode::NotImplemented
        );
        assert_eq!(
            bind_select_err("select sum(name) from users;"),
            ErrorCode::NotImplemented
        );
        assert_eq!(
            bind_select_err("select sum(*) from users;"),
            ErrorCode::NotImplemented
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_select_non_key_predicate_becomes_residual() {
        let select = bind_select("select id from users where name = 'x';");
        assert_eq!(select.residual.len(), 1);
        assert_eq!(select.residual[0].attr, 1);
        assert_eq!(select.residual[0].op, ValueCompare::EQ);
        assert!(matches!(
            select.predicate,
            BoundPredicate::KeyRange { .. } | BoundPredicate::True
        ));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_select_key_predicate_plus_residual() {
        let select = bind_select("select id from users where id = 1 and name = 'x';");
        assert!(matches!(select.predicate, BoundPredicate::KeyEq { .. }));
        assert_eq!(select.residual.len(), 1);
        assert_eq!(select.residual[0].attr, 1);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_select_aggregate_with_residual() {
        let select = bind_select("select count(*) from users where id = 1 and name = 'x';");
        assert!(matches!(select.predicate, BoundPredicate::KeyEq { .. }));
        assert_eq!(select.residual.len(), 1);
        assert!(matches!(
            select.select_items[0],
            BoundSelectItem::Aggregate(_)
        ));
    }

    /// Timing probe for the statement-path cost discussion: how expensive is
    /// `Binder::bind` for TPC-C-shaped statements (literals, no params).
    /// Not a correctness test; prints per-bind cost.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn bind_bench() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let binder = Binder::new(Arc::new(TestMetaMgr::new(composite_schema())));
            let sqls = [
                "SELECT tenant_id, user_id, name FROM accounts WHERE tenant_id = 1 AND user_id = 2",
                "UPDATE accounts SET name = 'x' WHERE tenant_id = 1 AND user_id = 2",
                "INSERT INTO accounts (tenant_id, user_id, name) VALUES (1, 2, 'x')",
                "DELETE FROM accounts WHERE tenant_id = 1 AND user_id = 2",
            ];
            let parsed: Vec<StmtType> = sqls.iter().map(|sql| parse_stmt(sql)).collect();
            let iters = 20_000usize;
            let start = mudu_sys::time::instant_now();
            let mut bound_count = 0usize;
            for i in 0..iters {
                let stmt = parsed[i % parsed.len()].clone();
                let _bound = binder.bind(stmt, &()).await.unwrap();
                bound_count += 1;
            }
            let elapsed = start.elapsed();
            assert_eq!(bound_count, iters);
            println!(
                "bind_bench: {} binds in {:?} => {:.2} us/bind",
                iters,
                elapsed,
                elapsed.as_secs_f64() * 1e6 / iters as f64
            );
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
            let binder = Binder::new(Arc::new(TestMetaMgr::new(schema)));
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
}
