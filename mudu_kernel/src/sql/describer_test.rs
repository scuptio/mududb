// Miri cannot execute FFI calls into the tree-sitter C parser, which is
// used by SQLParser inside this module. Individual tests are skipped under
// Miri; describer behavior is still exercised by normal `cargo test`.
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
    use crate::contract::schema_column::SchemaColumn;
    use crate::contract::schema_table::SchemaTable;
    use crate::contract::table_desc::TableDesc;
    use crate::contract::table_info::TableInfo;
    use crate::sql::describer::Describer;
    use async_trait::async_trait;
    use mudu::common::id::OID;
    use mudu::common::result::RS;
    use mudu::error::ErrorCode;
    use mudu::mudu_error;
    use mudu_sys::sync::SMutex;
    use mudu_type::data_type::DataType;
    use mudu_type::data_type_info::DataTypeInfo;
    use mudu_type::type_family::TypeFamily;
    use sql_parser::ast::expr_name::ExprName;
    use sql_parser::ast::select_term::SelectTerm;
    use sql_parser::ast::stmt_insert::StmtInsert;
    use sql_parser::ast::stmt_select::StmtSelect;
    use sql_parser::ast::stmt_type::StmtCommand;
    use sql_parser::ast::stmt_type::StmtType;
    use std::collections::HashMap;
    use std::sync::Arc;

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

    fn meta_mgr() -> Arc<TestMetaMgr> {
        Arc::new(TestMetaMgr::new(schema()))
    }

    fn select_term(name: &str) -> SelectTerm {
        let mut field = ExprName::new();
        field.set_name(name.to_string());
        let mut term = SelectTerm::new();
        term.set_field(field);
        term
    }

    fn select(table: &str, terms: Vec<&str>) -> StmtType {
        let mut stmt = StmtSelect::new();
        stmt.set_table_reference(table.to_string());
        for term in terms {
            stmt.add_select_term(select_term(term));
        }
        StmtType::Select(stmt)
    }

    fn command() -> StmtType {
        StmtType::Command(StmtCommand::Insert(StmtInsert::new(
            "users".to_string(),
            vec![],
            vec![],
        )))
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn describe_command_returns_empty_descriptor() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let desc = Describer::describe(meta_mgr().as_ref(), command())
                .await
                .unwrap();
            assert!(desc.fields().is_empty());
        })
        .unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn describe_select_explicit_columns_projects_table_descriptor() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let desc = Describer::describe(meta_mgr().as_ref(), select("users", vec!["id"]))
                .await
                .unwrap();
            let fields = desc.fields();
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].name(), "id");
            assert_eq!(fields[0].type_family(), TypeFamily::I32);
            assert!(!fields[0].nullable());
        })
        .unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn describe_select_all_columns_returns_full_table_descriptor() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let desc =
                Describer::describe(meta_mgr().as_ref(), select("users", vec!["id", "name"]))
                    .await
                    .unwrap();
            let fields = desc.fields();
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name(), "id");
            assert_eq!(fields[0].type_family(), TypeFamily::I32);
            assert!(!fields[0].nullable());
            assert_eq!(fields[1].name(), "name");
            assert_eq!(fields[1].type_family(), TypeFamily::String);
            assert!(fields[1].nullable());
        })
        .unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn describe_select_missing_table_returns_entity_not_found_with_table_name() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let err = Describer::describe(meta_mgr().as_ref(), select("missing_table", vec!["id"]))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
            assert!(err.message().contains("missing_table"));
        })
        .unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn describe_select_missing_column_returns_entity_not_found_with_column_name() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let err =
                Describer::describe(meta_mgr().as_ref(), select("users", vec!["missing_col"]))
                    .await
                    .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
            assert!(err.message().contains("missing_col"));
        })
        .unwrap();
    }
}
