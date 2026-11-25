use super::*;

#[test]
fn invalid_partition_references_are_rejected_on_write_and_before_recovery_publication() {
    block_on(async {
        for case in 0..3 {
            let dir = temp_dir().join(format!("binding-reference-{}", mudu_utils::oid::gen_oid()));
            let mgr = MetaMgrImpl::new(&dir).await.unwrap();
            mgr.initialize().await.unwrap();
            let schema = test_schema();
            mgr.create_table(&schema).await.unwrap();
            let rule = PartitionRuleDesc::new_range("rule".into(), vec![TypeFamily::I32], vec![]);
            mgr.create_partition_rule(&rule).await.unwrap();
            let mut binding = TablePartitionBinding {
                table_id: schema.id(),
                rule_id: rule.oid,
                ref_attr_indices: vec![0],
            };
            match case {
                0 => binding.rule_id += 1,
                1 => binding.ref_attr_indices = vec![0, 1],
                _ => binding.ref_attr_indices = vec![schema.columns().len()],
            }
            let version = mgr.catalog_version();
            assert!(mgr.bind_table_partition(&binding).await.is_err());
            assert_eq!(mgr.catalog_version(), version);
            assert!(mgr
                .get_table_partition_binding(schema.id())
                .await
                .unwrap()
                .is_none());
            crate::partition_binding_catalog::write_partition_binding_to_catalog(
                &mgr.catalog_relation().unwrap().partition_binding_catalog,
                &binding,
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
            assert!(reopened.list_partition_rules().await.unwrap().is_empty());
        }
    });
}

fn fs_schema(id: u64, kind: FsTypeKind, queue: bool) -> SchemaClass {
    let mut field = SchemaColumn::new(
        "file".into(),
        TypeFamily::U128,
        DataTypeInfo::from_text(TypeFamily::U128, String::new()),
    );
    field.set_fs_binding(Some(FsColumnBinding::new(id, kind)));
    let mut columns = if queue {
        catalog::contract::queue::queue_system_columns()
    } else {
        vec![test_schema().columns()[0].clone()]
    };
    columns.push(field);
    let values = (1..columns.len()).collect();
    let mut schema = SchemaClass::new("bound_fs".into(), columns, vec![0], values);
    if queue {
        schema.set_kind(SchemaClassKind::Queue);
    }
    schema
}

#[test]
fn filesystem_references_require_existing_type_with_matching_kind() {
    block_on(async {
        for queue in [false, true] {
            for wrong_kind in [false, true] {
                let dir = temp_dir().join(format!("fs-reference-{}", mudu_utils::oid::gen_oid()));
                let mgr = MetaMgrImpl::new(&dir).await.unwrap();
                mgr.initialize().await.unwrap();
                let id = mgr.create_fs_type("files", FsTypeKind::File).await.unwrap();
                let schema = fs_schema(
                    if wrong_kind { id } else { id ^ 1 },
                    if wrong_kind {
                        FsTypeKind::Directory
                    } else {
                        FsTypeKind::File
                    },
                    queue,
                );
                let version = mgr.catalog_version();
                let result = if queue {
                    mgr.create_queue(&schema).await
                } else {
                    mgr.create_table(&schema).await
                };
                assert!(result.is_err());
                assert_eq!(mgr.catalog_version(), version);
                assert!(mgr.get_table_by_id(schema.id()).await.is_err());
                crate::schema_catalog::write_schema_to_catalog(
                    &mgr.catalog_relation().unwrap().schema_catalog,
                    &schema,
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

#[test]
fn valid_filesystem_reference_survives_reopen() {
    block_on(async {
        for queue in [false, true] {
            let dir = temp_dir().join(format!("valid-fs-reference-{}", mudu_utils::oid::gen_oid()));
            let mgr = MetaMgrImpl::new(&dir).await.unwrap();
            mgr.initialize().await.unwrap();
            let id = mgr.create_fs_type("files", FsTypeKind::File).await.unwrap();
            let schema = fs_schema(id, FsTypeKind::File, queue);
            if queue {
                mgr.create_queue(&schema).await.unwrap();
            } else {
                mgr.create_table(&schema).await.unwrap();
            }
            mgr.flush_dirty_pages().await.unwrap();
            drop(mgr);
            let reopened = MetaMgrImpl::new(&dir).await.unwrap();
            reopened.initialize().await.unwrap();
            assert!(reopened.get_table_by_id(schema.id()).await.is_ok());
            assert_eq!(
                reopened
                    .get_fs_type_by_id(id)
                    .await
                    .unwrap()
                    .unwrap()
                    .kind(),
                FsTypeKind::File
            );
        }
    });
}

#[test]
fn rule_updates_cannot_invalidate_bindings_and_rebinding_updates_live_view() {
    block_on(async {
        let dir = temp_dir().join(format!("rule-references-{}", mudu_utils::oid::gen_oid()));
        let mgr = MetaMgrImpl::new(&dir).await.unwrap();
        mgr.initialize().await.unwrap();
        let schema = test_schema();
        mgr.create_table(&schema).await.unwrap();
        let rule = PartitionRuleDesc::new_range("rule".into(), vec![TypeFamily::I32], vec![]);
        let other = PartitionRuleDesc::new_range("other".into(), vec![TypeFamily::I32], vec![]);
        mgr.create_partition_rule(&rule).await.unwrap();
        mgr.create_partition_rule(&other).await.unwrap();
        let mut binding = TablePartitionBinding {
            table_id: schema.id(),
            rule_id: rule.oid,
            ref_attr_indices: vec![0],
        };
        mgr.bind_table_partition(&binding).await.unwrap();
        let mut invalid = rule.clone();
        invalid.version += 1;
        invalid.key_types.push(TypeFamily::I32);
        let version = mgr.catalog_version();
        assert!(mgr.update_partition_rule(&invalid).await.is_err());
        assert_eq!(mgr.catalog_version(), version);
        assert_eq!(mgr.get_partition_rule_by_id(rule.oid).await.unwrap(), rule);
        binding.rule_id = other.oid;
        mgr.bind_table_partition(&binding).await.unwrap();
        assert_eq!(
            mgr.get_table_partition_binding(schema.id()).await.unwrap(),
            Some(binding.clone())
        );
        mgr.flush_dirty_pages().await.unwrap();
        drop(mgr);
        let reopened = MetaMgrImpl::new(&dir).await.unwrap();
        reopened.initialize().await.unwrap();
        assert_eq!(
            reopened
                .get_table_partition_binding(schema.id())
                .await
                .unwrap(),
            Some(binding)
        );
    });
}
