//! Tests for template-mode binding: slot ordering, classification, and
//! fill-time equivalence with immediate binding (`Binder::bind_ref`).
//!
//! Miri cannot execute the tree-sitter FFI behind SQL parsing, so every test
//! is skipped under Miri (same convention as `binder_test`).
#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use crate::contract::fs_type::{FsColumnBinding, FsTypeKind};
    use crate::contract::meta_mgr::MetaMgr;
    use crate::contract::schema_column::SchemaColumn;
    use crate::contract::schema_table::SchemaTable;
    use crate::server::test_meta_mgr::TestMetaMgr;
    use crate::sql::binder::Binder;
    use crate::sql::bound_template::{BoundTemplate, PlanClass, StmtTemplate};
    use crate::x_engine::api::DeltaOp;
    use mudu_contract::database::sql_params::SQLParams;
    use mudu_type::data_type::DataType;
    use mudu_type::data_type_info::DataTypeInfo;
    use mudu_type::data_type_param_numeric::DataTypeParamNumeric;
    use mudu_type::type_family::TypeFamily;
    use sql_parser::ast::parser::SQLParser;
    use sql_parser::ast::stmt_type::StmtType;
    use std::sync::Arc;

    fn column(name: &str, family: TypeFamily) -> SchemaColumn {
        SchemaColumn::new(
            name.to_string(),
            family,
            DataTypeInfo::from_opt_object(&DataType::default_for(family)),
        )
    }

    fn numeric_column(name: &str) -> SchemaColumn {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(9, 2));
        SchemaColumn::new(
            name.to_string(),
            TypeFamily::Numeric,
            DataTypeInfo::from_opt_object(&ty),
        )
    }

    fn accounts_schema() -> SchemaTable {
        SchemaTable::new(
            "accounts".to_string(),
            vec![
                column("tenant_id", TypeFamily::I32),
                column("user_id", TypeFamily::I32),
                column("name", TypeFamily::String),
            ],
            vec![0, 1],
            vec![2],
        )
    }

    fn counters_schema() -> SchemaTable {
        SchemaTable::new(
            "counters".to_string(),
            vec![
                column("id", TypeFamily::I32),
                column("count", TypeFamily::I32),
                column("note", TypeFamily::String),
            ],
            vec![0],
            vec![1, 2],
        )
    }

    fn numeric_counters_schema() -> SchemaTable {
        SchemaTable::new(
            "numeric_counters".to_string(),
            vec![column("id", TypeFamily::I32), numeric_column("total")],
            vec![0],
            vec![1],
        )
    }

    fn ledger_schema() -> SchemaTable {
        SchemaTable::new(
            "ledger".to_string(),
            vec![numeric_column("amount"), column("note", TypeFamily::String)],
            vec![0],
            vec![1],
        )
    }

    fn fs_docs_schema() -> SchemaTable {
        let mut doc = column("doc", TypeFamily::U128);
        doc.set_fs_binding(Some(FsColumnBinding::new(7, FsTypeKind::File)));
        SchemaTable::new(
            "fs_docs".to_string(),
            vec![column("id", TypeFamily::I32), doc],
            vec![0],
            vec![1],
        )
    }

    async fn meta_mgr() -> Arc<TestMetaMgr> {
        let mgr = Arc::new(TestMetaMgr::new());
        for schema in [
            accounts_schema(),
            counters_schema(),
            numeric_counters_schema(),
            ledger_schema(),
            fs_docs_schema(),
        ] {
            mgr.create_table(&schema).await.unwrap();
        }
        mgr
    }

    fn parse_stmt(sql: &str) -> StmtType {
        SQLParser::new().unwrap().parse(sql).unwrap().stmts()[0].clone()
    }

    async fn bind_template(binder: &Binder, sql: &str) -> BoundTemplate {
        binder
            .bind_template(&parse_stmt(sql))
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("expected a template for {sql:?}"))
    }

    /// Asserts that filling the template with `params` yields exactly the
    /// statement immediate binding produces for the same input.
    async fn assert_fill_matches_bind(binder: &Binder, sql: &str, params: &dyn SQLParams) {
        let stmt = parse_stmt(sql);
        let template = binder
            .bind_template(&stmt)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("expected a template for {sql:?}"));
        let filled = template.fill(params).unwrap();
        let bound = binder.bind_ref(&stmt, params).await.unwrap();
        assert_eq!(
            format!("{filled:?}"),
            format!("{bound:?}"),
            "template fill must match immediate bind for {sql:?}"
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn select_point_read_classified_and_slots_ordered() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let binder = Binder::new(meta_mgr().await);
            let template = bind_template(
                &binder,
                "select name from accounts where tenant_id = ? and user_id = ?",
            )
            .await;
            match template.classify() {
                PlanClass::PointRead { select } => assert_eq!(select, vec![2]),
                other => panic!("expected PointRead, got {other:?}"),
            }
            assert_eq!(template.slots.len(), 2);
            assert_eq!(template.slots[0].param_index, 0);
            assert_eq!(template.slots[1].param_index, 1);
            assert!(!template.slots[0].delta_operand);

            // Cross-parameter reuse: each fill matches an immediate bind with
            // the same parameters.
            assert_fill_matches_bind(
                &binder,
                "select name from accounts where tenant_id = ? and user_id = ?",
                &(1i32, 2i32),
            )
            .await;
            assert_fill_matches_bind(
                &binder,
                "select name from accounts where tenant_id = ? and user_id = ?",
                &(7i32, 9i32),
            )
            .await;
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn select_range_residual_aggregate_and_duplicate_projection_are_other() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let binder = Binder::new(meta_mgr().await);
            for (sql, params) in [
                (
                    "select name from accounts where tenant_id >= ?",
                    &(1i32,) as &dyn SQLParams,
                ),
                (
                    "select name from accounts where tenant_id = ? and name = ?",
                    &(1i32, "a".to_string()) as &dyn SQLParams,
                ),
                (
                    "select count(*) from accounts where tenant_id = ? and user_id = ?",
                    &(1i32, 2i32) as &dyn SQLParams,
                ),
                (
                    "select name, name from accounts where tenant_id = ? and user_id = ?",
                    &(1i32, 2i32) as &dyn SQLParams,
                ),
            ] {
                let template = bind_template(&binder, sql).await;
                assert!(
                    matches!(template.classify(), PlanClass::Other),
                    "expected Other for {sql:?}"
                );
                assert_fill_matches_bind(&binder, sql, params).await;
            }
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn update_mixed_absolute_and_delta_slots_in_order() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let binder = Binder::new(meta_mgr().await);
            let sql = "update counters set note = ?, count = count + ? where id = ?";
            let template = bind_template(&binder, sql).await;
            assert!(matches!(template.classify(), PlanClass::PointUpdate));
            // Slot order: SET items first (absolute, then delta operand),
            // then the WHERE key, matching immediate binding.
            assert_eq!(template.slots.len(), 3);
            assert_eq!(template.slots[0].param_index, 0);
            assert!(!template.slots[0].delta_operand);
            assert_eq!(template.slots[1].param_index, 1);
            assert!(template.slots[1].delta_operand);
            assert_eq!(template.slots[2].param_index, 2);
            assert!(!template.slots[2].delta_operand);
            let StmtTemplate::Update(update) = &template.stmt else {
                panic!("expected update template");
            };
            assert!(matches!(
                update.value[1].1,
                crate::sql::bound_template::SetValueTemplate::Delta {
                    op: DeltaOp::Add,
                    ..
                }
            ));

            assert_fill_matches_bind(&binder, sql, &("a".to_string(), 5i32, 1i32)).await;
            assert_fill_matches_bind(&binder, sql, &("b".to_string(), -3i32, 42i32)).await;
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn insert_multi_row_slots_are_row_major() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let binder = Binder::new(meta_mgr().await);
            let sql = "insert into accounts values (?, ?, ?), (?, ?, ?)";
            let template = bind_template(&binder, sql).await;
            assert!(matches!(template.classify(), PlanClass::PointInsert));
            assert_eq!(template.slots.len(), 6);
            for (index, slot) in template.slots.iter().enumerate() {
                assert_eq!(slot.param_index, index as u64);
            }

            assert_fill_matches_bind(
                &binder,
                sql,
                &(1i32, 2i32, "a".to_string(), 3i32, 4i32, "b".to_string()),
            )
            .await;
            assert_fill_matches_bind(
                &binder,
                sql,
                &(5i32, 6i32, "c".to_string(), 7i32, 8i32, "d".to_string()),
            )
            .await;
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn delete_is_templated_as_other() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let binder = Binder::new(meta_mgr().await);
            let sql = "delete from counters where id = ?";
            let template = bind_template(&binder, sql).await;
            assert!(matches!(template.classify(), PlanClass::Other));
            assert_eq!(template.slots.len(), 1);
            assert_fill_matches_bind(&binder, sql, &(1i32,)).await;
            assert_fill_matches_bind(&binder, sql, &(2i32,)).await;
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn numeric_string_params_fill_like_immediate_bind() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let binder = Binder::new(meta_mgr().await);
            // NUMERIC params travel type-erased as strings; the fill path must
            // coerce them exactly like immediate binding.
            assert_fill_matches_bind(
                &binder,
                "insert into ledger values (?, ?)",
                &("3.00".to_string(), "rent".to_string()),
            )
            .await;
            assert_fill_matches_bind(
                &binder,
                "insert into ledger values (?, ?)",
                &("-12.75".to_string(), "refund".to_string()),
            )
            .await;
            assert_fill_matches_bind(
                &binder,
                "select note from ledger where amount = ?",
                &("3.00".to_string(),),
            )
            .await;
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn delta_numeric_string_placeholder_accepted() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let binder = Binder::new(meta_mgr().await);
            let sql = "update numeric_counters set total = total + ? where id = ?";
            let template = bind_template(&binder, sql).await;
            assert!(matches!(template.classify(), PlanClass::PointUpdate));
            assert!(template.slots[0].delta_operand);
            assert_fill_matches_bind(&binder, sql, &("2.50".to_string(), 1i32)).await;
            assert_fill_matches_bind(&binder, sql, &("0.01".to_string(), 2i32)).await;
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn fs_bound_tables_are_classified_other() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let binder = Binder::new(meta_mgr().await);
            let insert_sql = "insert into fs_docs values (?, ?)";
            let template = bind_template(&binder, insert_sql).await;
            let StmtTemplate::Insert(insert) = &template.stmt else {
                panic!("expected insert template");
            };
            assert!(insert.has_fs_columns);
            assert!(matches!(template.classify(), PlanClass::Other));

            let update_sql = "update fs_docs set doc = ? where id = ?";
            let template = bind_template(&binder, update_sql).await;
            assert!(matches!(template.classify(), PlanClass::Other));
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn fill_errors_on_missing_parameter() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let binder = Binder::new(meta_mgr().await);
            let template = bind_template(&binder, "delete from counters where id = ?").await;
            let err = template.fill(&()).unwrap_err();
            assert!(err.to_string().contains("missing parameter 0"));
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn fill_rejects_non_integer_delta_parameter() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let binder = Binder::new(meta_mgr().await);
            let template = bind_template(
                &binder,
                "update counters set count = count + ? where id = ?",
            )
            .await;
            let err = template.fill(&(1.5f64, 1i32)).unwrap_err();
            assert!(err
                .to_string()
                .contains("expression updates are not implemented"));
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn ddl_statements_are_not_templated() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let binder = Binder::new(meta_mgr().await);
            assert!(binder
                .bind_template(&parse_stmt("create table t (id int primary key)"))
                .await
                .unwrap()
                .is_none());
        })
        .unwrap()
    }
}
