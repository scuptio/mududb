#[cfg(test)]
mod tests {
    use crate::contract::meta_mgr::MetaMgr;
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
    use mudu::error::ec::EC;
    use mudu::m_error;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::datum::DatumDyn;
    use mudu_type::dt_info::DTInfo;
    use mudu_type::dtp_numeric::DTPNumeric;
    use sql_parser::ast::parser::SQLParser;
    use sql_parser::ast::stmt_type::StmtType;
    use std::collections::HashMap;
    use std::sync::Arc;
use mudu_sys::sync::SMutex;

    struct TestMetaMgr {
        tables: SMutex<HashMap<OID, Arc<TableDesc>>>,
    }

    impl TestMetaMgr {
        fn new(schema: SchemaTable) -> Self {
            let table = TableInfo::new(schema).unwrap().table_desc().unwrap();
            let mut tables = HashMap::new();
            tables.insert(table.id(), table);
            Self {
                tables: SMutex::new(tables),
            }
        }
    }

    #[async_trait]
    impl MetaMgr for TestMetaMgr {
        async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
            self.tables
                .lock()
                .unwrap()
                .get(&oid)
                .cloned()
                .ok_or_else(|| m_error!(EC::NoSuchElement, format!("no such table {}", oid)))
        }

        async fn get_table_by_name(&self, name: &String) -> RS<Option<Arc<TableDesc>>> {
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
        SQLParser::new().parse(sql).unwrap().stmts()[0].clone()
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

    #[tokio::test]
    async fn bind_select_builds_key_eq_predicate() {
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
    }

    #[tokio::test]
    async fn bind_create_table_preserves_nullable_constraints() {
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
    }

    #[tokio::test]
    async fn bind_select_uses_key_prefix_eq_for_left_prefix_of_composite_primary_key() {
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
    }

    #[tokio::test]
    async fn bind_select_rejects_non_leftmost_composite_primary_key_equality() {
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
    }

    #[tokio::test]
    async fn bind_select_reverses_value_column_comparison() {
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
    }

    #[tokio::test]
    async fn bind_select_builds_range_predicate_from_placeholder() {
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
    }

    #[tokio::test]
    async fn bind_select_rejects_not_equal_predicate() {
        let err = binder()
            .bind(parse_stmt("select id from users where id != 1;"), &())
            .await
            .unwrap_err();

        assert!(err
            .to_string()
            .contains("not-equal predicates are not implemented"));
    }

    #[tokio::test]
    async fn bind_select_rejects_mixed_equality_and_range_predicates() {
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
    }

    #[tokio::test]
    async fn bind_insert_without_column_list_uses_schema_order() {
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
    }

    #[tokio::test]
    async fn bind_insert_allows_null_for_nullable_value_column() {
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
    }

    #[tokio::test]
    async fn bind_insert_rejects_null_for_primary_key() {
        let err = binder()
            .bind(
                parse_stmt("insert into users (id, name) values (null, 'alice');"),
                &(),
            )
            .await
            .unwrap_err();

        assert!(err.to_string().contains("NOT NULL"));
    }

    #[tokio::test]
    async fn bind_insert_rejects_null_for_not_null_value_column() {
        let err = not_null_value_binder()
            .bind(
                parse_stmt("insert into users (id, name) values (1, null);"),
                &(),
            )
            .await
            .unwrap_err();

        assert!(err.to_string().contains("NOT NULL"));
    }

    #[tokio::test]
    async fn bind_insert_accepts_multi_row_insert() {
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
    }

    #[tokio::test]
    async fn bind_insert_accepts_multi_row_insert_with_placeholders() {
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
    }

    #[tokio::test]
    async fn bind_insert_encodes_numeric_literal_into_declared_column_type() {
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
    }

    #[tokio::test]
    async fn bind_select_numeric_placeholder_uses_numeric_key_encoding() {
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
    }

    #[tokio::test]
    async fn bind_insert_rejects_column_size_mismatch() {
        let err = binder()
            .bind(
                parse_stmt("insert into users (id) values (1, 'alice');"),
                &(),
            )
            .await
            .unwrap_err();

        assert!(err.to_string().contains("insert column size mismatch"));
    }

    #[tokio::test]
    async fn bind_update_rejects_primary_key_updates() {
        let err = binder()
            .bind(parse_stmt("update users set id = 2 where id = 1;"), &())
            .await
            .unwrap_err();

        assert!(err
            .to_string()
            .contains("updating primary key columns is not implemented"));
    }

    #[tokio::test]
    async fn bind_update_rejects_expression_updates() {
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
    }

    #[tokio::test]
    async fn bind_delete_rejects_non_key_predicates() {
        let err = binder()
            .bind(parse_stmt("delete from users where name = 'alice';"), &())
            .await
            .unwrap_err();

        assert!(err
            .to_string()
            .contains("non-key predicates are not implemented"));
    }

    #[tokio::test]
    async fn bind_delete_requires_complete_composite_primary_key() {
        let err = composite_binder()
            .bind(parse_stmt("delete from accounts where tenant_id = 1;"), &())
            .await
            .unwrap_err();

        assert!(err.to_string().contains("complete primary key predicate"));
    }

    #[tokio::test]
    async fn bind_delete_accepts_complete_composite_primary_key() {
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
    }
}
