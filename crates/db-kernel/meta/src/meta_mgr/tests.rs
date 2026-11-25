#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use crate::fs_object::FS_OBJECT_TABLE_ID;
use ::catalog::contract::fs_type::{FsColumnBinding, FsTypeKind};
use ::catalog::contract::meta_mgr::{CatalogSnapshot, MetaMgr};
use ::catalog::contract::schema_class::{SchemaClass, SchemaClassKind};
use ::catalog::contract::schema_column::SchemaColumn;
use mudu_sys::env_var::temp_dir;
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::type_family::TypeFamily;
use std::future::Future;

use super::*;
use catalog::contract::derive_oid::derive_fs_id;

fn block_on<F>(fut: F) -> F::Output
where
    F: Future + 'static,
    F::Output: 'static,
{
    mudu_sys::task::async_::block_on_tokio_current_thread(fut).unwrap()
}

fn test_schema() -> SchemaClass {
    SchemaClass::new(
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
    assert_eq!(table.name(), schema.class_name());
    Ok(())
}

#[test]
fn catalog_snapshot_captures_and_restores_definitions() {
    block_on(async move {
        let r = _catalog_snapshot_captures_and_restores_definitions().await;
        assert!(r.is_ok());
    });
}
async fn _catalog_snapshot_captures_and_restores_definitions() -> RS<()> {
    let src_dir = temp_dir().join(format!("meta_snapshot_src_{}", mudu_utils::oid::gen_oid()));
    let src = Arc::new(MetaMgrImpl::new(src_dir).await?);
    src.register_global()?;
    src.initialize().await?;
    let schema = test_schema();
    src.create_table(&schema).await?;
    let rule = PartitionRuleDesc::new_range("snap_rule".to_string(), vec![TypeFamily::I32], vec![]);
    src.create_partition_rule(&rule).await?;
    src.bind_table_partition(&TablePartitionBinding {
        table_id: schema.id(),
        rule_id: rule.oid,
        ref_attr_indices: vec![0],
    })
    .await?;
    src.create_fs_type("snap_fs", FsTypeKind::File).await?;

    // Capture and round-trip through the wire encoding.
    let snapshot = src.capture_catalog().await?;
    let bytes = rmp_serde::to_vec(&snapshot)
        .map_err(|e| mudu_error!(ER::Encode, "encode catalog snapshot", e))?;
    let snapshot: CatalogSnapshot = rmp_serde::from_slice(&bytes)
        .map_err(|e| mudu_error!(ER::Decode, "decode catalog snapshot", e))?;

    // Restore into a fresh manager.
    let dst_dir = temp_dir().join(format!("meta_snapshot_dst_{}", mudu_utils::oid::gen_oid()));
    let dst = Arc::new(MetaMgrImpl::new(dst_dir).await?);
    dst.register_global()?;
    dst.initialize().await?;
    dst.restore_catalog(&snapshot).await?;

    // The definitions (plus the two built-in tables) landed.
    assert_eq!(dst.list_schemas().await?.len(), snapshot.schemas.len() + 2);
    assert_eq!(dst.list_partition_rules().await?.len(), 1);
    assert_eq!(dst.list_partition_bindings().await?.len(), 1);
    assert_eq!(dst.list_fs_types().await?.len(), 1);
    assert!(dst
        .get_table_by_name("mududb", "meta_recovery_t")
        .await?
        .is_some());
    assert!(dst.get_fs_type_by_name("snap_fs").await?.is_some());
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
    assert_eq!(table.name(), schema.class_name());

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
    assert_eq!(file_id, derive_fs_id("file_t"));
    assert_eq!(dir_id, derive_fs_id("dir_t"));

    let by_name = mgr.get_fs_type_by_name("file_t").await?;
    assert_eq!(
        by_name,
        Some(FsTypeDesc::new(
            "file_t".to_string(),
            derive_fs_id("file_t"),
            FsTypeKind::File
        ))
    );
    let by_id = mgr.get_fs_type_by_id(derive_fs_id("dir_t")).await?;
    assert_eq!(
        by_id,
        Some(FsTypeDesc::new(
            "dir_t".to_string(),
            derive_fs_id("dir_t"),
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
    assert!(mgr
        .get_fs_type_by_id(derive_fs_id("file_t"))
        .await?
        .is_none());
    assert_eq!(mgr.list_fs_types().await?.len(), 1);

    let missing = mgr.drop_fs_type("file_t").await;
    assert_eq!(missing.unwrap_err().ec(), ER::ObjectNotFound);
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
    let schema = SchemaClass::new(
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
        Some(FsTypeDesc::new(
            "file_t".to_string(),
            derive_fs_id("file_t"),
            FsTypeKind::File
        ))
    );
    let dir_type = reopened.get_fs_type_by_id(derive_fs_id("dir_t")).await?;
    assert_eq!(
        dir_type,
        Some(FsTypeDesc::new(
            "dir_t".to_string(),
            derive_fs_id("dir_t"),
            FsTypeKind::Directory
        ))
    );
    let next_id = reopened.create_fs_type("next_t", FsTypeKind::File).await?;
    assert_eq!(next_id, derive_fs_id("next_t"));
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
    let schema = SchemaClass::new(
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
    assert_eq!(err.ec(), ER::ObjectAlreadyExists);
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
        .get_table_by_name(DEFAULT_SCHEMA, schema.class_name())
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

fn test_schema_in(schema_ns: &str, class_name: &str) -> SchemaClass {
    let mut schema = SchemaClass::new(
        class_name.to_string(),
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
    assert_eq!(dup.unwrap_err().ec(), ER::ObjectAlreadyExists);

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
    // (oid, class_name, columns, key_indices, value_indices); the serde
    // default must decode them into the default schema.
    let schema = test_schema_in("wallet", "users");
    let legacy_payload = rmp_serde::to_vec(&(
        schema.id(),
        schema.class_name().clone(),
        schema.columns().clone(),
        schema.key_indices().clone(),
        schema.value_indices().clone(),
    ))
    .map_err(|e| mudu_error!(ER::Encode, "encode legacy schema row error", e))?;
    let decoded = decode_schema_catalog_value(&legacy_payload)?;
    assert_eq!(decoded.schema(), DEFAULT_SCHEMA);
    assert_eq!(decoded.class_name(), schema.class_name());
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

fn test_queue_schema(name: &str) -> SchemaClass {
    let mut schema = SchemaClass::new(
        name.to_string(),
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
    schema.set_kind(SchemaClassKind::Queue);
    schema
}

#[test]
fn meta_mgr_queue_create_get_list_and_drop() {
    block_on(async move {
        let r = _meta_mgr_queue_create_get_list_and_drop().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_queue_create_get_list_and_drop() -> RS<()> {
    let mgr = new_test_mgr("queue_crud").await?;
    assert_eq!(mgr.catalog_version(), 0);

    // A schema without the queue kind is rejected.
    let not_a_queue = test_schema();
    let err = mgr.create_queue(&not_a_queue).await.unwrap_err();
    assert_eq!(err.ec(), ER::InvalidArgument);

    let queue = test_queue_schema("events");
    mgr.create_queue(&queue).await?;
    assert_eq!(mgr.catalog_version(), 1);

    // The queue resolves through both the queue getter (kind-filtered) and
    // the regular table lookup (queues are table-shaped storage objects).
    let desc = mgr
        .get_queue(DEFAULT_SCHEMA, "events")
        .await?
        .ok_or_else(|| mudu_error!(ER::Internal, "missing queue"))?;
    assert_eq!(desc.id(), queue.id());
    let as_table = mgr
        .get_table_by_name(DEFAULT_SCHEMA, "events")
        .await?
        .ok_or_else(|| mudu_error!(ER::Internal, "missing queue as table"))?;
    assert_eq!(as_table.id(), queue.id());

    let queues = mgr.list_queues().await?;
    assert_eq!(queues.len(), 1);
    assert_eq!(queues[0].id(), queue.id());
    assert_eq!(queues[0].kind(), SchemaClassKind::Queue);
    // list_schemas stays the full catalog (it drives bootstrap/replay).
    assert!(mgr
        .list_schemas()
        .await?
        .iter()
        .any(|s| s.id() == queue.id()));

    // Tables never resolve through the queue getters.
    let table = test_schema_in(DEFAULT_SCHEMA, "plain_t");
    mgr.create_table(&table).await?;
    assert!(mgr.get_queue(DEFAULT_SCHEMA, "plain_t").await?.is_none());
    assert_eq!(mgr.list_queues().await?.len(), 1);

    // Queues share the (schema, name) namespace with tables, both ways.
    let dup = mgr.create_queue(&test_queue_schema("plain_t")).await;
    assert_eq!(dup.unwrap_err().ec(), ER::ObjectAlreadyExists);
    let dup_table = test_schema_in(DEFAULT_SCHEMA, "events");
    let dup = mgr.create_table(&dup_table).await;
    assert_eq!(dup.unwrap_err().ec(), ER::ObjectAlreadyExists);

    // A table oid is not droppable through drop_queue.
    let err = mgr.drop_queue(table.id()).await.unwrap_err();
    assert_eq!(err.ec(), ER::ObjectNotFound);

    mgr.drop_queue(queue.id()).await?;
    assert_eq!(mgr.catalog_version(), 3);
    assert!(mgr.get_queue(DEFAULT_SCHEMA, "events").await?.is_none());
    assert!(mgr.list_queues().await?.is_empty());
    assert!(mgr
        .get_table_by_name(DEFAULT_SCHEMA, "events")
        .await?
        .is_none());

    let missing = mgr.drop_queue(queue.id()).await;
    assert_eq!(missing.unwrap_err().ec(), ER::ObjectNotFound);
    Ok(())
}

#[test]
fn meta_mgr_queue_search_path_falls_back_to_default_schema() {
    block_on(async move {
        let r = _meta_mgr_queue_search_path_falls_back_to_default_schema().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_queue_search_path_falls_back_to_default_schema() -> RS<()> {
    let mgr = new_test_mgr("queue_fallback").await?;
    let queue = test_queue_schema("jobs");
    mgr.create_queue(&queue).await?;

    // The queue lives in the default schema; an application schema reaches
    // it through the search-path fallback, exactly like tables.
    let resolved = mgr
        .get_queue("wallet", "jobs")
        .await?
        .ok_or_else(|| mudu_error!(ER::Internal, "fallback missed queue"))?;
    assert_eq!(resolved.id(), queue.id());
    assert!(mgr.get_queue("wallet", "missing").await?.is_none());
    Ok(())
}

#[test]
fn meta_mgr_recovers_queue_catalog_after_reopen() {
    block_on(async move {
        let r = _meta_mgr_recovers_queue_catalog_after_reopen().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_recovers_queue_catalog_after_reopen() -> RS<()> {
    let dir = temp_dir().join(format!(
        "meta_mgr_queue_replay_{}",
        mudu_utils::oid::gen_oid()
    ));
    let mgr = Arc::new(MetaMgrImpl::new(&dir).await?);
    mgr.register_global()?;
    mgr.initialize().await?;
    let queue = test_queue_schema("events");
    mgr.create_queue(&queue).await?;
    let schema_catalog = mgr.catalog_relation()?.schema_catalog;
    schema_catalog.flush_wal_async().await?;
    drop(mgr);

    let reopened = MetaMgrImpl::new(&dir).await?;
    reopened.initialize().await?;
    // The kind tag survives catalog replay: the queue still resolves through
    // the kind-filtered getters.
    let queues = reopened.list_queues().await?;
    assert_eq!(queues.len(), 1);
    assert_eq!(queues[0].id(), queue.id());
    assert_eq!(queues[0].kind(), SchemaClassKind::Queue);
    let desc = reopened
        .get_queue(DEFAULT_SCHEMA, "events")
        .await?
        .ok_or_else(|| mudu_error!(ER::Internal, "missing queue after replay"))?;
    assert_eq!(desc.id(), queue.id());
    Ok(())
}

#[test]
fn meta_mgr_legacy_catalog_row_without_kind_decodes_as_table() {
    block_on(async move {
        let r = _meta_mgr_legacy_catalog_row_without_kind_decodes_as_table().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_legacy_catalog_row_without_kind_decodes_as_table() -> RS<()> {
    use crate::schema_catalog::decode_schema_catalog_value;

    // Rows written between the `schema` and `kind` introductions hold six
    // positional fields; the serde default must decode them as plain tables.
    let schema = test_schema_in("wallet", "users");
    let legacy_payload = rmp_serde::to_vec(&(
        schema.id(),
        schema.class_name().clone(),
        schema.columns().clone(),
        schema.key_indices().clone(),
        schema.value_indices().clone(),
        schema.schema().clone(),
    ))
    .map_err(|e| mudu_error!(ER::Encode, "encode legacy schema row error", e))?;
    let decoded = decode_schema_catalog_value(&legacy_payload)?;
    assert_eq!(decoded.kind(), SchemaClassKind::Table);
    assert!(!decoded.is_queue());
    assert_eq!(decoded.schema(), "wallet");
    assert_eq!(decoded.class_name(), schema.class_name());
    Ok(())
}

#[test]
fn meta_mgr_update_partition_rule_reaches_peers() {
    block_on(async move {
        let r = _meta_mgr_update_partition_rule_reaches_peers().await;
        assert!(r.is_ok());
    });
}
async fn _meta_mgr_update_partition_rule_reaches_peers() -> RS<()> {
    let dir = temp_dir().join(format!(
        "meta_mgr_rule_update_peer_{}",
        mudu_utils::oid::gen_oid()
    ));
    let mgr1 = Arc::new(MetaMgrImpl::new(&dir).await?);
    mgr1.register_global()?;
    mgr1.initialize().await?;
    let mgr2 = Arc::new(MetaMgrImpl::new(&dir).await?);
    mgr2.register_global()?;
    mgr2.initialize().await?;

    use ::catalog::contract::partition_rule::{PartitionBound, RangePartitionDef};
    let rule = PartitionRuleDesc::new_range_with_oid(
        77,
        "upd_rule".to_string(),
        vec![TypeFamily::I32],
        vec![RangePartitionDef::new_with_oid(
            700,
            "p_a".to_string(),
            PartitionBound::Unbounded,
            PartitionBound::Unbounded,
        )],
    );
    mgr1.create_partition_rule(&rule).await?;
    let mut updated = rule.clone();
    updated.version += 1;
    mgr1.update_partition_rule(&updated).await?;
    assert_eq!(mgr1.get_partition_rule_by_id(77).await?.version, 2);
    assert_eq!(mgr2.get_partition_rule_by_id(77).await?.version, 2);
    Ok(())
}

mod fs_identity;

mod drop_binding;

mod identity;

mod initialization;

mod references;
