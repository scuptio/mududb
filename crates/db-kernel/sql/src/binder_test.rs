// Miri cannot execute FFI calls into the tree-sitter C parser, which is
// used by SQLParser inside this module. Individual tests are skipped under
// Miri; binder behavior is still exercised by normal `cargo test`.
#![allow(clippy::todo, clippy::unimplemented)]

mod aggregate_schema;
mod ddl_fs_copy;
mod dml;
mod partition;
mod update_delta;

use crate::binder::Binder;
use crate::bound_stmt::{
    AggregateFunc, BoundCommand, BoundPredicate, BoundQuery, BoundSelectItem, BoundSetValue,
    BoundStmt,
};
use ::catalog::contract::app_schema::DEFAULT_SCHEMA;
use ::catalog::contract::fs_type::{FsColumnBinding, FsTypeDesc, FsTypeKind};
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::partition_rule::{PartitionBound, PartitionRuleDesc, RangePartitionDef};
use ::catalog::contract::partition_rule_binding::PartitionPlacement;
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::table_info::TableInfo;
use ::xengine::api::DeltaOp;
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
            .ok_or_else(|| mudu_error!(ErrorCode::EntityNotFound, format!("no such table {}", oid)))
    }

    async fn get_table_by_name(&self, schema: &str, name: &str) -> RS<Option<Arc<TableDesc>>> {
        let tables = self.tables.lock().unwrap();
        let exact = tables
            .values()
            .find(|table| table.schema() == schema && table.name() == name)
            .cloned();
        // Mirror the catalog search path: exact (schema, name) hit wins,
        // then fall back to the default schema.
        if exact.is_some() || schema == DEFAULT_SCHEMA {
            return Ok(exact);
        }
        Ok(tables
            .values()
            .find(|table| table.schema() == DEFAULT_SCHEMA && table.name() == name)
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

fn binder_arc(meta: Arc<TestMetaMgr>) -> Binder {
    Binder::new(meta, DEFAULT_SCHEMA.to_string())
}

fn binder_arc_with_schema(meta: Arc<TestMetaMgr>, current_schema: &str) -> Binder {
    Binder::new(meta, current_schema.to_string())
}

fn binder() -> Binder {
    binder_arc(Arc::new(TestMetaMgr::new(schema())))
}

fn composite_binder() -> Binder {
    binder_arc(Arc::new(TestMetaMgr::new(composite_schema())))
}

fn numeric_binder() -> Binder {
    binder_arc(Arc::new(TestMetaMgr::new(numeric_schema())))
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
    binder_arc(Arc::new(TestMetaMgr::new(SchemaTable::new(
        "users".to_string(),
        vec![id, name],
        vec![0],
        vec![1],
    ))))
}
