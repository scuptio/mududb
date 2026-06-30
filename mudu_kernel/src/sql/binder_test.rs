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

    use crate::contract::meta_mgr::MetaMgr;
    use crate::contract::partition_rule::{PartitionBound, PartitionRuleDesc, RangePartitionDef};
    use crate::contract::partition_rule_binding::PartitionPlacement;
    use crate::contract::schema_column::SchemaColumn;
    use crate::contract::schema_table::SchemaTable;
    use crate::contract::table_desc::TableDesc;
    use crate::contract::table_info::TableInfo;
    use crate::sql::binder::Binder;
    use crate::sql::bound_stmt::{BoundCommand, BoundPredicate, BoundQuery, BoundStmt};
    use async_trait::async_trait;
    use mudu::common::id::OID;
    use mudu::common::result::RS;
    use mudu::data_type::numeric::Numeric;
    use mudu::error::ErrorCode;
    use mudu::mudu_error;
    use mudu_sys::sync::SMutex;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::datum::DatumDyn;
    use mudu_type::dt_info::DTInfo;
    use mudu_type::dtp_numeric::DTPNumeric;
    use sql_parser::ast::parser::SQLParser;
    use sql_parser::ast::stmt_type::StmtType;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct TestMetaMgr {
        tables: SMutex<HashMap<OID, Arc<TableDesc>>>,
        rules: SMutex<HashMap<String, PartitionRuleDesc>>,
    }

    impl TestMetaMgr {
        fn new(schema: SchemaTable) -> Self {
            let table = TableInfo::new(schema).unwrap().table_desc().unwrap();
            let mut tables = HashMap::new();
            tables.insert(table.id(), table);
            Self {
                tables: SMutex::new(tables),
                rules: SMutex::new(HashMap::new()),
            }
        }

        fn with_rule(schema: SchemaTable, rule: PartitionRuleDesc) -> Self {
            let mgr = Self::new(schema);
            mgr.rules.lock().unwrap().insert(rule.name.clone(), rule);
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
    }

    fn schema() -> SchemaTable {
        SchemaTable::new(
            "users".to_string(),
            vec![
                SchemaColumn::new(
                    "id".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
                ),
                SchemaColumn::new(
                    "name".to_string(),
                    DatTypeID::String,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::String)),
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
                    DatTypeID::I32,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
                ),
                SchemaColumn::new(
                    "user_id".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
                ),
                SchemaColumn::new(
                    "name".to_string(),
                    DatTypeID::String,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::String)),
                ),
            ],
            vec![0, 1],
            vec![2],
        )
    }

    fn numeric_schema() -> SchemaTable {
        let amount_type = DatType::from_numeric(DTPNumeric::new(9, 2));
        let note_type = DatType::default_for(DatTypeID::String);
        SchemaTable::new(
            "ledger".to_string(),
            vec![
                SchemaColumn::new(
                    "amount".to_string(),
                    DatTypeID::Numeric,
                    DTInfo::from_opt_object(&amount_type),
                ),
                SchemaColumn::new(
                    "note".to_string(),
                    DatTypeID::String,
                    DTInfo::from_opt_object(&note_type),
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
            DatTypeID::I32,
            DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
        );
        let mut name = SchemaColumn::new(
            "name".to_string(),
            DatTypeID::String,
            DTInfo::from_opt_object(&DatType::default_for(DatTypeID::String)),
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
            assert_eq!(select.select_attrs, vec![0]);
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
            let amount_type = DatType::from_numeric(DTPNumeric::new(9, 2));
            let note_type = DatType::default_for(DatTypeID::String);

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
            let amount_type = DatType::from_numeric(DTPNumeric::new(9, 2));
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
            vec![DatTypeID::I32],
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
                    DatTypeID::I32,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
                ),
                SchemaColumn::new(
                    "order_id".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
                ),
                SchemaColumn::new(
                    "amount".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
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
            assert_eq!(rule.rule.key_types, vec![DatTypeID::I64]);
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
}
