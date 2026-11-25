use super::*;

async fn bound(queue: bool) -> (std::path::PathBuf, Arc<MetaMgrImpl>, SchemaClass) {
    let dir = temp_dir().join(format!("drop-binding-{}", mudu_utils::oid::gen_oid()));
    let mgr = Arc::new(MetaMgrImpl::new(&dir).await.unwrap());
    mgr.register_global().unwrap();
    mgr.initialize().await.unwrap();
    let schema = if queue {
        test_queue_schema("bound")
    } else {
        test_schema()
    };
    if queue {
        mgr.create_queue(&schema).await.unwrap();
    } else {
        mgr.create_table(&schema).await.unwrap();
    }
    let rule = PartitionRuleDesc::new_range("rule".into(), vec![TypeFamily::I32], vec![]);
    mgr.create_partition_rule(&rule).await.unwrap();
    mgr.bind_table_partition(&TablePartitionBinding {
        table_id: schema.id(),
        rule_id: rule.oid,
        ref_attr_indices: vec![0],
    })
    .await
    .unwrap();
    (dir, mgr, schema)
}

#[test]
fn dropping_objects_removes_binding_from_durable_catalog_and_peer_views() {
    block_on(async {
        for queue in [false, true] {
            let (dir, mgr, schema) = bound(queue).await;
            let peer = Arc::new(MetaMgrImpl::new(&dir).await.unwrap());
            peer.register_global().unwrap();
            peer.initialize().await.unwrap();
            assert!(peer
                .get_table_partition_binding(schema.id())
                .await
                .unwrap()
                .is_some());
            let version = mgr.catalog_version();
            if queue {
                mgr.drop_queue(schema.id()).await.unwrap();
            } else {
                mgr.drop_table(schema.id()).await.unwrap();
            }
            assert_eq!(mgr.catalog_version(), version + 1);
            for view in [&mgr, &peer] {
                assert!(view
                    .get_table_partition_binding(schema.id())
                    .await
                    .unwrap()
                    .is_none());
                assert!(view.get_table_by_id(schema.id()).await.is_err());
            }
            mgr.flush_dirty_pages().await.unwrap();
            drop(peer);
            drop(mgr);
            let reopened = MetaMgrImpl::new(&dir).await.unwrap();
            reopened.initialize().await.unwrap();
            assert!(reopened
                .get_table_partition_binding(schema.id())
                .await
                .unwrap()
                .is_none());
            if queue {
                reopened.create_queue(&schema).await.unwrap();
            } else {
                reopened.create_table(&schema).await.unwrap();
            }
            assert!(reopened
                .get_table_partition_binding(schema.id())
                .await
                .unwrap()
                .is_none());
        }
    });
}

#[test]
fn interrupted_schema_drop_leaves_binding_that_can_be_cleaned_after_reopen() {
    block_on(async {
        for queue in [false, true] {
            let (dir, mgr, schema) = bound(queue).await;
            assert!(mgr
                .remove_orphan_partition_binding(schema.id())
                .await
                .is_err());
            assert!(mgr
                .get_table_partition_binding(schema.id())
                .await
                .unwrap()
                .is_some());
            // Stop after the first durable deletion substep, before binding
            // deletion or in-memory broadcast, then recover from real files.
            crate::schema_catalog::delete_schema_from_catalog(
                &mgr.catalog_relation().unwrap().schema_catalog,
                schema.id(),
                mgr.next_catalog_xid(),
            )
            .await
            .unwrap();
            mgr.flush_dirty_pages().await.unwrap();
            drop(mgr);
            let reopened = Arc::new(MetaMgrImpl::new(&dir).await.unwrap());
            reopened.register_global().unwrap();
            reopened.initialize().await.unwrap();
            assert!(reopened.get_table_by_id(schema.id()).await.is_err());
            assert!(reopened
                .get_table_partition_binding(schema.id())
                .await
                .unwrap()
                .is_some());
            reopened
                .remove_orphan_partition_binding(schema.id())
                .await
                .unwrap();
            let version = reopened.catalog_version();
            reopened
                .remove_orphan_partition_binding(schema.id())
                .await
                .unwrap();
            assert_eq!(reopened.catalog_version(), version);
            reopened.flush_dirty_pages().await.unwrap();
            drop(reopened);
            let recovered = MetaMgrImpl::new(&dir).await.unwrap();
            recovered.initialize().await.unwrap();
            assert!(recovered
                .get_table_partition_binding(schema.id())
                .await
                .unwrap()
                .is_none());
        }
    });
}
