use super::*;

fn alias(schema: &SchemaClass, oid: OID, name: &str) -> SchemaClass {
    let mut result = SchemaClass::new_with_oid(
        oid,
        name.into(),
        schema.columns().clone(),
        schema.key_indices().clone(),
        schema.value_indices().clone(),
    );
    result.set_schema(schema.schema().clone());
    result.set_kind(schema.kind());
    result
}

#[test]
fn table_and_queue_creation_reject_oid_aliases_before_persisting() {
    block_on(async {
        for queue in [false, true] {
            let dir = temp_dir().join(format!("oid-alias-{}", mudu_utils::oid::gen_oid()));
            let mgr = MetaMgrImpl::new(&dir).await.unwrap();
            mgr.initialize().await.unwrap();
            let original = if queue {
                test_queue_schema("original")
            } else {
                test_schema()
            };
            if queue {
                mgr.create_queue(&original).await.unwrap();
            } else {
                mgr.create_table(&original).await.unwrap();
            }
            let version = mgr.catalog_version();
            let table_alias = alias(&test_schema(), original.id(), "table_alias");
            let queue_alias = alias(&test_queue_schema("q"), original.id(), "queue_alias");
            assert!(mgr.create_table(&table_alias).await.is_err());
            assert!(mgr.create_queue(&queue_alias).await.is_err());
            assert_eq!(mgr.catalog_version(), version);
            mgr.flush_dirty_pages().await.unwrap();
            drop(mgr);
            let mgr = MetaMgrImpl::new(&dir).await.unwrap();
            mgr.initialize().await.unwrap();
            assert_eq!(
                mgr.get_table_by_id(original.id()).await.unwrap().name(),
                original.class_name()
            );
            for conflict in [table_alias, queue_alias] {
                assert!(mgr
                    .get_table_exact(conflict.schema(), conflict.class_name())
                    .await
                    .unwrap()
                    .is_none());
            }
        }
    });
}

#[test]
fn partition_rule_updates_preserve_identity_and_advance_one_version() {
    block_on(async {
        let dir = temp_dir().join(format!("rule-identity-{}", mudu_utils::oid::gen_oid()));
        let mgr = MetaMgrImpl::new(&dir).await.unwrap();
        mgr.initialize().await.unwrap();
        let rule = PartitionRuleDesc::new_range("original".into(), vec![TypeFamily::I32], vec![]);
        mgr.create_partition_rule(&rule).await.unwrap();
        let mut alias = rule.clone();
        alias.name = "alias".into();
        assert!(mgr.create_partition_rule(&alias).await.is_err());
        let version = mgr.catalog_version();
        for case in 0..5 {
            let mut conflict = rule.clone();
            match case {
                0 => conflict.oid += 1,
                1 => conflict.name = "alias".into(),
                2 => conflict.version = 0,
                3 => conflict.version += 2,
                _ => conflict.key_types = vec![TypeFamily::I64],
            }
            assert!(mgr.update_partition_rule(&conflict).await.is_err());
            assert_eq!(mgr.catalog_version(), version);
        }
        let mut updated = rule.clone();
        updated.version += 1;
        updated.key_types = vec![TypeFamily::I64];
        mgr.update_partition_rule(&updated).await.unwrap();
        let version = mgr.catalog_version();
        mgr.update_partition_rule(&updated).await.unwrap();
        assert_eq!(mgr.catalog_version(), version);
        mgr.flush_dirty_pages().await.unwrap();
        drop(mgr);
        let mgr = MetaMgrImpl::new(&dir).await.unwrap();
        mgr.initialize().await.unwrap();
        assert_eq!(
            mgr.get_partition_rule_by_id(rule.oid).await.unwrap(),
            updated
        );
        assert!(mgr
            .get_partition_rule_by_name("alias")
            .await
            .unwrap()
            .is_none());
    });
}

#[test]
fn recovery_rejects_duplicate_names_instead_of_choosing_one_identity() {
    block_on(async {
        for rule_catalog in [false, true] {
            let dir = temp_dir().join(format!("duplicate-name-{}", mudu_utils::oid::gen_oid()));
            let mgr = MetaMgrImpl::new(&dir).await.unwrap();
            mgr.initialize().await.unwrap();
            if rule_catalog {
                let rule =
                    PartitionRuleDesc::new_range("duplicate".into(), vec![TypeFamily::I32], vec![]);
                mgr.create_partition_rule(&rule).await.unwrap();
                let mut conflict = rule.clone();
                conflict.oid += 1;
                crate::partition_rule_catalog::write_partition_rule_to_catalog(
                    &mgr.catalog_relation().unwrap().partition_rule_catalog,
                    &conflict,
                    mgr.next_catalog_xid(),
                )
                .await
                .unwrap();
            } else {
                let schema = test_schema();
                mgr.create_table(&schema).await.unwrap();
                let conflict = alias(&schema, schema.id() + 1, schema.class_name());
                crate::schema_catalog::write_schema_to_catalog(
                    &mgr.catalog_relation().unwrap().schema_catalog,
                    &conflict,
                    mgr.next_catalog_xid(),
                )
                .await
                .unwrap();
            }
            mgr.flush_dirty_pages().await.unwrap();
            drop(mgr);
            let reopened = MetaMgrImpl::new(&dir).await.unwrap();
            assert_eq!(
                reopened.initialize().await.unwrap_err().ec(),
                ER::CorruptedData
            );
        }
    });
}

#[test]
fn truncated_or_oversized_catalog_identity_keys_return_errors() {
    for size in [0, 15, 17] {
        let key = vec![0; size];
        assert!(crate::schema_catalog::decode_schema_catalog_key(&key).is_err());
        assert!(crate::partition_rule_catalog::decode_partition_rule_catalog_key(&key).is_err());
    }
}
