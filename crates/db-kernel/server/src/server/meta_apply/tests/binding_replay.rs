use super::*;
use catalog::contract::queue::queue_dlq_schema;

fn create(schema: &SchemaClass, binding: Option<TablePartitionBinding>) -> MetaOp {
    if schema.is_queue() {
        MetaOp::CreateQueue {
            schema: schema.clone(),
            binding,
        }
    } else {
        MetaOp::CreateTable {
            schema: schema.clone(),
            binding,
        }
    }
}

#[test]
fn create_rejects_foreign_binding_and_missing_rule_before_persisting_objects() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        for queue in [false, true] {
            for foreign in [false, true] {
                let (dir, meta) = manager().await;
                let target = schema(queue);
                let other = SchemaClass::new(
                    "other".into(),
                    schema(false).columns().clone(),
                    vec![0],
                    vec![],
                );
                meta.create_table(&other).await.unwrap();
                let rule =
                    PartitionRuleDesc::new_range("rule".into(), vec![TypeFamily::I32], vec![]);
                if foreign {
                    meta.create_partition_rule(&rule).await.unwrap();
                }
                let binding = TablePartitionBinding {
                    table_id: if foreign { other.id() } else { target.id() },
                    rule_id: rule.oid,
                    ref_attr_indices: vec![0],
                };
                let version = meta.catalog_version();
                assert!(apply_meta_op(meta.as_ref(), create(&target, Some(binding)))
                    .await
                    .is_err());
                assert_eq!(meta.catalog_version(), version);
                meta.flush_dirty_pages().await.unwrap();
                drop(meta);
                let meta = meta::meta_mgr_factory::MetaMgrFactory::create(dir)
                    .await
                    .unwrap();
                meta.initialize().await.unwrap();
                assert!(meta.get_table_by_id(target.id()).await.is_err());
                if queue {
                    assert!(meta
                        .get_table_by_id(queue_dlq_schema(&target).id())
                        .await
                        .is_err());
                }
                assert!(meta
                    .get_table_partition_binding(other.id())
                    .await
                    .unwrap()
                    .is_none());
                assert!(meta
                    .get_table_partition_binding(target.id())
                    .await
                    .unwrap()
                    .is_none());
            }
        }
    })
    .unwrap();
}

#[test]
fn conflicting_or_omitted_binding_cannot_create_missing_dlq_on_retry() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        for queue in [false, true] {
            for omitted in [false, true] {
                let (_, meta) = manager().await;
                let target = schema(queue);
                if queue {
                    meta.create_queue(&target).await.unwrap();
                } else {
                    meta.create_table(&target).await.unwrap();
                }
                let rule =
                    PartitionRuleDesc::new_range("rule".into(), vec![TypeFamily::I32], vec![]);
                meta.create_partition_rule(&rule).await.unwrap();
                let existing = TablePartitionBinding {
                    table_id: target.id(),
                    rule_id: rule.oid,
                    ref_attr_indices: vec![0],
                };
                meta.bind_table_partition(&existing).await.unwrap();
                let mut conflict = existing.clone();
                conflict.ref_attr_indices = vec![1];
                let version = meta.catalog_version();
                let intent = if omitted { None } else { Some(conflict) };
                assert!(apply_meta_op(meta.as_ref(), create(&target, intent))
                    .await
                    .is_err());
                assert_eq!(meta.catalog_version(), version);
                assert_eq!(
                    meta.get_table_partition_binding(target.id()).await.unwrap(),
                    Some(existing)
                );
                if queue {
                    assert!(meta
                        .get_table_by_id(queue_dlq_schema(&target).id())
                        .await
                        .is_err());
                }
            }
        }
    })
    .unwrap();
}

#[test]
fn invalid_partition_column_references_do_not_create_objects() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        for queue in [false, true] {
            let (_, meta) = manager().await;
            let target = schema(queue);
            let rule = PartitionRuleDesc::new_range("rule".into(), vec![TypeFamily::I32], vec![]);
            meta.create_partition_rule(&rule).await.unwrap();
            for indices in [vec![], vec![0, 0], vec![target.columns().len()]] {
                let binding = TablePartitionBinding {
                    table_id: target.id(),
                    rule_id: rule.oid,
                    ref_attr_indices: indices,
                };
                let version = meta.catalog_version();
                assert!(apply_meta_op(meta.as_ref(), create(&target, Some(binding)))
                    .await
                    .is_err());
                assert_eq!(meta.catalog_version(), version);
                assert!(meta.get_table_by_id(target.id()).await.is_err());
                if queue {
                    assert!(meta
                        .get_table_by_id(queue_dlq_schema(&target).id())
                        .await
                        .is_err());
                }
            }
        }
    })
    .unwrap();
}
