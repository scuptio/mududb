use super::*;

#[test]
fn late_catalog_failure_does_not_publish_earlier_rows_and_can_retry_after_repair() {
    block_on(async {
        let dir = temp_dir().join(format!("init-failure-{}", mudu_utils::oid::gen_oid()));
        let mgr = MetaMgrImpl::new(&dir).await.unwrap();
        mgr.initialize().await.unwrap();
        let schema = test_schema();
        mgr.create_table(&schema).await.unwrap();
        let rule = PartitionRuleDesc::new_range("rule".into(), vec![TypeFamily::I32], vec![]);
        mgr.create_partition_rule(&rule).await.unwrap();
        mgr.bind_table_partition(&TablePartitionBinding {
            table_id: schema.id(),
            rule_id: rule.oid,
            ref_attr_indices: vec![0],
        })
        .await
        .unwrap();
        let bad = FsTypeDesc::new("bad".into(), derive_fs_id("bad") ^ 1, FsTypeKind::File);
        crate::fs_type_catalog::write_fs_type_to_catalog(
            &mgr.catalog_relation().unwrap().fs_type,
            &bad,
            mgr.next_catalog_xid(),
        )
        .await
        .unwrap();
        mgr.flush_dirty_pages().await.unwrap();
        drop(mgr);
        let reopened = MetaMgrImpl::new(&dir).await.unwrap();
        assert!(reopened.initialize().await.is_err());
        assert!(reopened.catalog_relation().is_err());
        assert!(reopened.list_schemas().await.unwrap().is_empty());
        assert!(reopened.list_partition_rules().await.unwrap().is_empty());
        assert!(reopened
            .get_table_partition_binding(schema.id())
            .await
            .unwrap()
            .is_none());
        let relation = crate::fs_type_catalog::open_fs_type_catalog(dir.to_str().unwrap(), None)
            .await
            .unwrap();
        crate::fs_type_catalog::delete_fs_type_from_catalog(
            &relation,
            "bad",
            reopened.next_catalog_xid(),
        )
        .await
        .unwrap();
        relation.flush_dirty_pages().await.unwrap();
        drop(relation);
        reopened.initialize().await.unwrap();
        assert!(reopened.get_table_by_id(schema.id()).await.is_ok());
        assert!(reopened
            .get_table_partition_binding(schema.id())
            .await
            .unwrap()
            .is_some());
        assert_eq!(
            reopened.get_partition_rule_by_id(rule.oid).await.unwrap(),
            rule
        );
    });
}

#[test]
fn uninitialized_peer_receives_no_broadcast_and_concurrent_initializers_serialize() {
    block_on(async {
        let dir = temp_dir().join(format!("init-peer-{}", mudu_utils::oid::gen_oid()));
        let mgr = Arc::new(MetaMgrImpl::new(&dir).await.unwrap());
        mgr.register_global().unwrap();
        mgr.initialize().await.unwrap();
        let peer = Arc::new(MetaMgrImpl::new(&dir).await.unwrap());
        peer.register_global().unwrap();
        let schema = test_schema();
        mgr.create_table(&schema).await.unwrap();
        let tenant = test_schema_in("tenant", FS_OBJECT_TABLE_NAME);
        mgr.create_table(&tenant).await.unwrap();
        assert!(peer.list_schemas().await.unwrap().is_empty());
        assert_eq!(peer.catalog_version(), 0);
        let (a, b) = mudu_sys::tokio::join!(peer.initialize(), peer.initialize());
        a.unwrap();
        b.unwrap();
        assert!(peer.get_table_by_id(schema.id()).await.is_ok());
        assert!(peer.get_table_by_id(tenant.id()).await.is_ok());
        assert!(peer.get_table_by_id(FS_OBJECT_TABLE_ID).await.is_ok());
        mgr.drop_table(schema.id()).await.unwrap();
        assert!(peer.get_table_by_id(schema.id()).await.is_err());
        let version = peer.catalog_version();
        peer.initialize().await.unwrap();
        assert_eq!(peer.catalog_version(), version);
    });
}

#[test]
fn persisted_user_records_cannot_replace_builtin_names_or_oids() {
    block_on(async {
        for builtin in [fs_object_schema(), queue_consumer_schema()] {
            for by_name in [false, true] {
                let dir = temp_dir().join(format!("init-builtin-{}", mudu_utils::oid::gen_oid()));
                let mgr = MetaMgrImpl::new(&dir).await.unwrap();
                mgr.initialize().await.unwrap();
                let template = test_schema();
                let oid = if by_name { template.id() } else { builtin.id() };
                let name = if by_name {
                    builtin.class_name().clone()
                } else {
                    "alias".into()
                };
                let conflict = SchemaClass::new_with_oid(
                    oid,
                    name,
                    template.columns().clone(),
                    template.key_indices().clone(),
                    template.value_indices().clone(),
                );
                crate::schema_catalog::write_schema_to_catalog(
                    &mgr.catalog_relation().unwrap().schema_catalog,
                    &conflict,
                    mgr.next_catalog_xid(),
                )
                .await
                .unwrap();
                mgr.flush_dirty_pages().await.unwrap();
                drop(mgr);
                let reopened = MetaMgrImpl::new(&dir).await.unwrap();
                assert_eq!(
                    reopened.initialize().await.unwrap_err().ec(),
                    ER::CorruptedData
                );
                assert!(reopened.list_schemas().await.unwrap().is_empty());
            }
        }
    });
}
