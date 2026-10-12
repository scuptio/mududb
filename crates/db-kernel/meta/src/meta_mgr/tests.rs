#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use crate::fs_object::FS_OBJECT_TABLE_ID;
use ::catalog::contract::fs_type::{FsColumnBinding, FsTypeKind};
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::schema_table::SchemaTable;
use mudu_sys::env_var::temp_dir;
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::type_family::TypeFamily;
use std::future::Future;

use super::*;

fn block_on<F>(fut: F) -> F::Output
where
    F: Future + 'static,
    F::Output: 'static,
{
    mudu_sys::task::async_::block_on_tokio_current_thread(fut).unwrap()
}

fn test_schema() -> SchemaTable {
    SchemaTable::new(
        "meta_recovery_t".to_string(),
        vec![
            SchemaColumn::new(
                "id".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_text(TypeFamily::I32, String::new()),
            ),
            SchemaColumn::new(
                "v".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_text(TypeFamily::I32, String::new()),
            ),
        ],
        vec![0],
        vec![1],
    )
}

#[test]
fn meta_mgr_recovers_schema_catalog_after_reopen() {
    block_on(async move {
        let r = _meta_mgr_recovers_schema_catalog_after_reopen().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_recovers_schema_catalog_after_reopen() -> RS<()> {
    let dir = temp_dir().join(format!("meta_mgr_catalog_{}", mudu_utils::oid::gen_oid()));
    let initial_dir = dir.clone();
    let mgr = Arc::new(MetaMgrImpl::new(initial_dir).await?);
    mgr.register_global()?;
    mgr.initialize().await?;
    let _mgr = mgr.clone();
    let schema = test_schema();
    let _schema = schema.clone();
    _mgr.create_table(&_schema).await?;
    let schema_catalog = mgr.catalog_relation()?.schema_catalog;
    assert_eq!(load_schemas_from_catalog(&schema_catalog).await?.len(), 1);
    schema_catalog.flush_wal_async().await?;
    drop(mgr);

    let reopened = MetaMgrImpl::new(dir).await?;
    reopened.initialize().await?;
    let schema_id = schema.id();
    let table = reopened.get_table_by_id(schema_id).await?;
    assert_eq!(table.name(), schema.table_name());
    Ok(())
}

#[test]
fn meta_mgr_broadcasts_ddl_to_peer_instances() {
    block_on(async move {
        let r = _meta_mgr_broadcasts_ddl_to_peer_instances().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_broadcasts_ddl_to_peer_instances() -> RS<()> {
    let dir = temp_dir().join(format!("meta_mgr_peer_{}", mudu_utils::oid::gen_oid()));
    let mgr1 = Arc::new(MetaMgrImpl::new(&dir).await?);
    mgr1.register_global()?;
    mgr1.initialize().await?;
    let mgr2 = Arc::new(MetaMgrImpl::new(&dir).await?);
    mgr2.register_global()?;
    mgr2.initialize().await?;

    let schema = test_schema();
    mgr1.create_table(&schema).await?;
    let table = mgr2.get_table_by_id(schema.id()).await?;
    assert_eq!(table.name(), schema.table_name());

    mgr2.drop_table(schema.id()).await?;
    assert!(mgr1.get_table_by_id(schema.id()).await.is_err());
    Ok(())
}

#[test]
fn meta_mgr_fs_type_create_query_and_drop() {
    block_on(async move {
        let r = _meta_mgr_fs_type_create_query_and_drop().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_fs_type_create_query_and_drop() -> RS<()> {
    let dir = temp_dir().join(format!("meta_mgr_fs_type_{}", mudu_utils::oid::gen_oid()));
    let mgr = Arc::new(MetaMgrImpl::new(dir).await?);
    mgr.register_global()?;
    mgr.initialize().await?;

    let file_id = mgr.create_fs_type("file_t", FsTypeKind::File).await?;
    let dir_id = mgr.create_fs_type("dir_t", FsTypeKind::Directory).await?;
    assert_eq!(file_id, 1);
    assert_eq!(dir_id, 2);

    let by_name = mgr.get_fs_type_by_name("file_t").await?;
    assert_eq!(
        by_name,
        Some(FsTypeDesc::new("file_t".to_string(), 1, FsTypeKind::File))
    );
    let by_id = mgr.get_fs_type_by_id(2).await?;
    assert_eq!(
        by_id,
        Some(FsTypeDesc::new(
            "dir_t".to_string(),
            2,
            FsTypeKind::Directory
        ))
    );
    assert!(mgr.get_fs_type_by_name("missing").await?.is_none());
    assert!(mgr.get_fs_type_by_id(0).await?.is_none());
    assert_eq!(mgr.list_fs_types().await?.len(), 2);

    let dup = mgr.create_fs_type("file_t", FsTypeKind::File).await;
    assert_eq!(dup.unwrap_err().ec(), ER::AlreadyExists);

    mgr.drop_fs_type("file_t").await?;
    assert!(mgr.get_fs_type_by_name("file_t").await?.is_none());
    assert!(mgr.get_fs_type_by_id(1).await?.is_none());
    assert_eq!(mgr.list_fs_types().await?.len(), 1);

    let missing = mgr.drop_fs_type("file_t").await;
    assert_eq!(missing.unwrap_err().ec(), ER::EntityNotFound);
    Ok(())
}

#[test]
fn meta_mgr_drop_fs_type_rejects_still_referenced_type() {
    block_on(async move {
        let r = _meta_mgr_drop_fs_type_rejects_still_referenced_type().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_drop_fs_type_rejects_still_referenced_type() -> RS<()> {
    let dir = temp_dir().join(format!(
        "meta_mgr_fs_type_ref_{}",
        mudu_utils::oid::gen_oid()
    ));
    let mgr = Arc::new(MetaMgrImpl::new(dir).await?);
    mgr.register_global()?;
    mgr.initialize().await?;

    let fs_id = mgr.create_fs_type("photo_fs", FsTypeKind::File).await?;

    // Create a table with an FS-bound column referencing the type.
    let mut photo = SchemaColumn::new(
        "photo".to_string(),
        TypeFamily::U128,
        DataTypeInfo::from_text(TypeFamily::U128, String::new()),
    );
    photo.set_fs_binding(Some(FsColumnBinding::new(fs_id, FsTypeKind::File)));
    let schema = SchemaTable::new(
        "product".to_string(),
        vec![
            SchemaColumn::new(
                "id".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_text(TypeFamily::I32, String::new()),
            ),
            photo,
        ],
        vec![0],
        vec![1],
    );
    mgr.create_table(&schema).await?;

    // Dropping a still-referenced type fails and keeps the type.
    let err = mgr.drop_fs_type("photo_fs").await.unwrap_err();
    assert_eq!(err.ec(), ER::InvalidState);
    assert!(err.to_string().contains("product.photo"));
    assert!(mgr.get_fs_type_by_name("photo_fs").await?.is_some());

    // After dropping the table, dropping the type succeeds.
    mgr.drop_table(schema.id()).await?;
    mgr.drop_fs_type("photo_fs").await?;
    assert!(mgr.get_fs_type_by_name("photo_fs").await?.is_none());
    Ok(())
}

#[test]
fn meta_mgr_recovers_fs_type_catalog_after_reopen() {
    block_on(async move {
        let r = _meta_mgr_recovers_fs_type_catalog_after_reopen().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_recovers_fs_type_catalog_after_reopen() -> RS<()> {
    let dir = temp_dir().join(format!(
        "meta_mgr_fs_type_replay_{}",
        mudu_utils::oid::gen_oid()
    ));
    let mgr = Arc::new(MetaMgrImpl::new(&dir).await?);
    mgr.register_global()?;
    mgr.initialize().await?;
    mgr.create_fs_type("file_t", FsTypeKind::File).await?;
    mgr.create_fs_type("dir_t", FsTypeKind::Directory).await?;
    let fs_type_catalog = mgr.catalog_relation()?.fs_type;
    assert_eq!(load_fs_types_from_catalog(&fs_type_catalog).await?.len(), 2);
    fs_type_catalog.flush_wal_async().await?;
    drop(mgr);

    let reopened = MetaMgrImpl::new(dir).await?;
    reopened.initialize().await?;
    let file = reopened.get_fs_type_by_name("file_t").await?;
    assert_eq!(
        file,
        Some(FsTypeDesc::new("file_t".to_string(), 1, FsTypeKind::File))
    );
    let dir_type = reopened.get_fs_type_by_id(2).await?;
    assert_eq!(
        dir_type,
        Some(FsTypeDesc::new(
            "dir_t".to_string(),
            2,
            FsTypeKind::Directory
        ))
    );
    let next_id = reopened.create_fs_type("next_t", FsTypeKind::File).await?;
    assert_eq!(next_id, 3);
    Ok(())
}

#[test]
fn meta_mgr_registers_fs_object_system_table() {
    block_on(async move {
        let r = _meta_mgr_registers_fs_object_system_table().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_registers_fs_object_system_table() -> RS<()> {
    let dir = temp_dir().join(format!("meta_mgr_fs_object_{}", mudu_utils::oid::gen_oid()));
    let mgr = Arc::new(MetaMgrImpl::new(dir).await?);
    mgr.register_global()?;
    mgr.initialize().await?;

    let by_name = mgr
        .get_table_by_name(DEFAULT_SCHEMA, FS_OBJECT_TABLE_NAME)
        .await?;
    let desc = by_name.ok_or_else(|| mudu_error!(ER::Internal, "missing _fs_object"))?;
    assert_eq!(desc.id(), FS_OBJECT_TABLE_ID);
    let by_id = mgr.get_table_by_id(FS_OBJECT_TABLE_ID).await?;
    assert_eq!(by_id.name(), FS_OBJECT_TABLE_NAME);
    assert!(mgr
        .list_schemas()
        .await?
        .iter()
        .any(|schema| schema.id() == FS_OBJECT_TABLE_ID));

    // A user table named `_fs_object` fails as a duplicate.
    let schema = SchemaTable::new(
        FS_OBJECT_TABLE_NAME.to_string(),
        vec![SchemaColumn::new(
            "id".to_string(),
            TypeFamily::I32,
            DataTypeInfo::from_text(TypeFamily::I32, String::new()),
        )],
        vec![0],
        vec![],
    );
    let err = mgr.create_table(&schema).await.unwrap_err();
    assert_eq!(err.ec(), ER::EntityAlreadyExists);
    Ok(())
}

#[test]
fn meta_mgr_catalog_version_bumps_on_ddl_only() {
    block_on(async move {
        let r = _meta_mgr_catalog_version_bumps_on_ddl_only().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_catalog_version_bumps_on_ddl_only() -> RS<()> {
    let dir = temp_dir().join(format!(
        "meta_mgr_catalog_version_{}",
        mudu_utils::oid::gen_oid()
    ));
    let mgr = Arc::new(MetaMgrImpl::new(dir).await?);
    mgr.register_global()?;
    mgr.initialize().await?;
    assert_eq!(mgr.catalog_version(), 0);

    let schema = test_schema();
    mgr.create_table(&schema).await?;
    assert_eq!(mgr.catalog_version(), 1);

    // Non-DDL reads must not advance the version.
    let _ = mgr
        .get_table_by_name(DEFAULT_SCHEMA, schema.table_name())
        .await?;
    let _ = mgr.get_table_by_id(schema.id()).await?;
    let _ = mgr.list_schemas().await?;
    let _ = mgr.list_partition_rules().await?;
    let _ = mgr.list_fs_types().await?;
    assert_eq!(mgr.catalog_version(), 1);

    let rule = PartitionRuleDesc::new_range("ver_rule".to_string(), vec![TypeFamily::I32], vec![]);
    mgr.create_partition_rule(&rule).await?;
    assert_eq!(mgr.catalog_version(), 2);

    let binding = TablePartitionBinding {
        table_id: schema.id(),
        rule_id: rule.oid,
        ref_attr_indices: vec![0],
    };
    mgr.bind_table_partition(&binding).await?;
    assert_eq!(mgr.catalog_version(), 3);

    mgr.create_fs_type("ver_fs", FsTypeKind::File).await?;
    assert_eq!(mgr.catalog_version(), 4);
    mgr.drop_fs_type("ver_fs").await?;
    assert_eq!(mgr.catalog_version(), 5);

    mgr.drop_table(schema.id()).await?;
    assert_eq!(mgr.catalog_version(), 6);
    Ok(())
}

#[test]
fn meta_mgr_catalog_version_consistent_across_peer_instances() {
    block_on(async move {
        let r = _meta_mgr_catalog_version_consistent_across_peer_instances().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_catalog_version_consistent_across_peer_instances() -> RS<()> {
    let dir = temp_dir().join(format!(
        "meta_mgr_version_peer_{}",
        mudu_utils::oid::gen_oid()
    ));
    let mgr1 = Arc::new(MetaMgrImpl::new(&dir).await?);
    mgr1.register_global()?;
    mgr1.initialize().await?;
    let mgr2 = Arc::new(MetaMgrImpl::new(&dir).await?);
    mgr2.register_global()?;
    mgr2.initialize().await?;
    assert_eq!(mgr1.catalog_version(), 0);
    assert_eq!(mgr2.catalog_version(), 0);

    // DDL issued on one instance fans out through the registry and bumps
    // every instance on the same path exactly once.
    let schema = test_schema();
    mgr1.create_table(&schema).await?;
    assert_eq!(mgr1.catalog_version(), 1);
    assert_eq!(mgr2.catalog_version(), 1);

    mgr2.drop_table(schema.id()).await?;
    assert_eq!(mgr1.catalog_version(), 2);
    assert_eq!(mgr2.catalog_version(), 2);
    Ok(())
}

fn test_schema_in(schema_ns: &str, table_name: &str) -> SchemaTable {
    let mut schema = SchemaTable::new(
        table_name.to_string(),
        vec![
            SchemaColumn::new(
                "id".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_text(TypeFamily::I32, String::new()),
            ),
            SchemaColumn::new(
                "v".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_text(TypeFamily::I32, String::new()),
            ),
        ],
        vec![0],
        vec![1],
    );
    schema.set_schema(schema_ns.to_string());
    schema
}

async fn new_test_mgr(label: &str) -> RS<Arc<MetaMgrImpl>> {
    let dir = temp_dir().join(format!("meta_mgr_{}_{}", label, mudu_utils::oid::gen_oid()));
    let mgr = Arc::new(MetaMgrImpl::new(dir).await?);
    mgr.register_global()?;
    mgr.initialize().await?;
    Ok(mgr)
}

#[test]
fn meta_mgr_same_named_tables_coexist_across_schemas() {
    block_on(async move {
        let r = _meta_mgr_same_named_tables_coexist_across_schemas().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_same_named_tables_coexist_across_schemas() -> RS<()> {
    let mgr = new_test_mgr("ns_coexist").await?;

    let default_users = test_schema_in(DEFAULT_SCHEMA, "users");
    let wallet_users = test_schema_in("wallet", "users");
    mgr.create_table(&default_users).await?;
    // Same name in another schema is not a collision.
    mgr.create_table(&wallet_users).await?;
    // Exact re-create in the same schema still collides.
    let dup = mgr.create_table(&test_schema_in("wallet", "users")).await;
    assert_eq!(dup.unwrap_err().ec(), ER::EntityAlreadyExists);

    let resolved_default = mgr
        .get_table_by_name(DEFAULT_SCHEMA, "users")
        .await?
        .ok_or_else(|| mudu_error!(ER::Internal, "missing default users"))?;
    assert_eq!(resolved_default.id(), default_users.id());
    assert_eq!(resolved_default.schema(), DEFAULT_SCHEMA);
    let resolved_wallet = mgr
        .get_table_by_name("wallet", "users")
        .await?
        .ok_or_else(|| mudu_error!(ER::Internal, "missing wallet users"))?;
    assert_eq!(resolved_wallet.id(), wallet_users.id());
    assert_eq!(resolved_wallet.schema(), "wallet");
    Ok(())
}

#[test]
fn meta_mgr_search_path_falls_back_to_default_schema() {
    block_on(async move {
        let r = _meta_mgr_search_path_falls_back_to_default_schema().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_search_path_falls_back_to_default_schema() -> RS<()> {
    let mgr = new_test_mgr("ns_fallback").await?;

    let default_users = test_schema_in(DEFAULT_SCHEMA, "users");
    mgr.create_table(&default_users).await?;

    // The application schema does not own `users`: the search path falls
    // back to the default schema.
    let resolved = mgr
        .get_table_by_name("wallet", "users")
        .await?
        .ok_or_else(|| mudu_error!(ER::Internal, "fallback missed default users"))?;
    assert_eq!(resolved.id(), default_users.id());
    assert_eq!(resolved.schema(), DEFAULT_SCHEMA);
    // The exact lookup never falls back (DDL collision semantics).
    assert!(mgr.get_table_exact("wallet", "users").await?.is_none());
    assert!(mgr
        .get_table_exact(DEFAULT_SCHEMA, "users")
        .await?
        .is_some());
    // Unknown names miss in every schema.
    assert!(mgr.get_table_by_name("wallet", "missing").await?.is_none());
    Ok(())
}

#[test]
fn meta_mgr_current_schema_shadows_default_schema() {
    block_on(async move {
        let r = _meta_mgr_current_schema_shadows_default_schema().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_current_schema_shadows_default_schema() -> RS<()> {
    let mgr = new_test_mgr("ns_shadow").await?;

    let default_users = test_schema_in(DEFAULT_SCHEMA, "users");
    mgr.create_table(&default_users).await?;
    // CREATE lands in the application schema even though the default
    // schema owns a same-named table (shadowing, not a collision).
    let wallet_users = test_schema_in("wallet", "users");
    mgr.create_table(&wallet_users).await?;
    let resolved = mgr
        .get_table_by_name("wallet", "users")
        .await?
        .ok_or_else(|| mudu_error!(ER::Internal, "missing wallet users"))?;
    assert_eq!(resolved.id(), wallet_users.id());
    // The default schema still resolves its own table.
    let resolved_default = mgr
        .get_table_by_name(DEFAULT_SCHEMA, "users")
        .await?
        .ok_or_else(|| mudu_error!(ER::Internal, "missing default users"))?;
    assert_eq!(resolved_default.id(), default_users.id());
    // Dropping the shadow reveals the default table again.
    mgr.drop_table(wallet_users.id()).await?;
    let resolved = mgr
        .get_table_by_name("wallet", "users")
        .await?
        .ok_or_else(|| mudu_error!(ER::Internal, "fallback missed default users"))?;
    assert_eq!(resolved.id(), default_users.id());
    Ok(())
}

#[test]
fn meta_mgr_recovers_schema_ownership_after_reopen() {
    block_on(async move {
        let r = _meta_mgr_recovers_schema_ownership_after_reopen().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_recovers_schema_ownership_after_reopen() -> RS<()> {
    let dir = temp_dir().join(format!("meta_mgr_ns_replay_{}", mudu_utils::oid::gen_oid()));
    let mgr = Arc::new(MetaMgrImpl::new(&dir).await?);
    mgr.register_global()?;
    mgr.initialize().await?;
    let default_users = test_schema_in(DEFAULT_SCHEMA, "users");
    let wallet_users = test_schema_in("wallet", "users");
    mgr.create_table(&default_users).await?;
    mgr.create_table(&wallet_users).await?;
    let schema_catalog = mgr.catalog_relation()?.schema_catalog;
    schema_catalog.flush_wal_async().await?;
    drop(mgr);

    let reopened = MetaMgrImpl::new(&dir).await?;
    reopened.initialize().await?;
    let resolved_wallet = reopened
        .get_table_by_name("wallet", "users")
        .await?
        .ok_or_else(|| mudu_error!(ER::Internal, "missing wallet users"))?;
    assert_eq!(resolved_wallet.id(), wallet_users.id());
    assert_eq!(resolved_wallet.schema(), "wallet");
    let resolved_default = reopened
        .get_table_by_name(DEFAULT_SCHEMA, "users")
        .await?
        .ok_or_else(|| mudu_error!(ER::Internal, "missing default users"))?;
    assert_eq!(resolved_default.id(), default_users.id());
    // The per-schema collision scope survives the reload as well.
    let dup = reopened
        .create_table(&test_schema_in("vote", "users"))
        .await;
    assert!(dup.is_ok());
    Ok(())
}

#[test]
fn meta_mgr_legacy_catalog_row_decodes_into_default_schema() {
    block_on(async move {
        let r = _meta_mgr_legacy_catalog_row_decodes_into_default_schema().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_legacy_catalog_row_decodes_into_default_schema() -> RS<()> {
    use crate::schema_catalog::decode_schema_catalog_value;

    // Catalog rows written before the `schema` field existed hold only
    // (oid, table_name, columns, key_indices, value_indices); the serde
    // default must decode them into the default schema.
    let schema = test_schema_in("wallet", "users");
    let legacy_payload = rmp_serde::to_vec(&(
        schema.id(),
        schema.table_name().clone(),
        schema.columns().clone(),
        schema.key_indices().clone(),
        schema.value_indices().clone(),
    ))
    .map_err(|e| mudu_error!(ER::Encode, "encode legacy schema row error", e))?;
    let decoded = decode_schema_catalog_value(&legacy_payload)?;
    assert_eq!(decoded.schema(), DEFAULT_SCHEMA);
    assert_eq!(decoded.table_name(), schema.table_name());
    Ok(())
}

#[test]
fn meta_mgr_fs_object_visible_from_application_schema() {
    block_on(async move {
        let r = _meta_mgr_fs_object_visible_from_application_schema().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_fs_object_visible_from_application_schema() -> RS<()> {
    let mgr = new_test_mgr("ns_fs_object").await?;
    // `_fs_object` is registered in the default schema; applications
    // reach it through the search-path fallback.
    let resolved = mgr
        .get_table_by_name("wallet", FS_OBJECT_TABLE_NAME)
        .await?
        .ok_or_else(|| mudu_error!(ER::Internal, "missing _fs_object"))?;
    assert_eq!(resolved.id(), FS_OBJECT_TABLE_ID);
    assert_eq!(resolved.schema(), DEFAULT_SCHEMA);
    Ok(())
}
