use super::*;
use catalog::contract::queue::queue_dlq_schema;

#[test]
fn control_drop_reopen_and_recreate_does_not_inherit_old_binding() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        for queue in [false, true] {
            let (dir, meta) = manager().await;
            let object = schema(queue);
            let rule = PartitionRuleDesc::new_range("rule".into(), vec![TypeFamily::I32], vec![]);
            meta.create_partition_rule(&rule).await.unwrap();
            let binding = Some(TablePartitionBinding {
                table_id: object.id(),
                rule_id: rule.oid,
                ref_attr_indices: vec![0],
            });
            let create = if queue {
                MetaOp::CreateQueue {
                    schema: object.clone(),
                    binding,
                }
            } else {
                MetaOp::CreateTable {
                    schema: object.clone(),
                    binding,
                }
            };
            apply_meta_op(meta.as_ref(), create).await.unwrap();
            if queue {
                meta.bind_table_partition(&TablePartitionBinding {
                    table_id: queue_dlq_schema(&object).id(),
                    rule_id: rule.oid,
                    ref_attr_indices: vec![0],
                })
                .await
                .unwrap();
            }
            let drop_op = if queue {
                MetaOp::DropQueue { oid: object.id() }
            } else {
                MetaOp::DropTable { oid: object.id() }
            };
            apply_meta_op(meta.as_ref(), drop_op.clone()).await.unwrap();
            meta.flush_dirty_pages().await.unwrap();
            drop(meta);
            let meta = meta::meta_mgr_factory::MetaMgrFactory::create(dir)
                .await
                .unwrap();
            meta.initialize().await.unwrap();
            let version = meta.catalog_version();
            apply_meta_op(meta.as_ref(), drop_op).await.unwrap();
            assert_eq!(meta.catalog_version(), version);
            assert!(meta
                .get_table_partition_binding(object.id())
                .await
                .unwrap()
                .is_none());
            if queue {
                assert!(meta
                    .get_table_partition_binding(queue_dlq_schema(&object).id())
                    .await
                    .unwrap()
                    .is_none());
            }
            let create = if queue {
                MetaOp::CreateQueue {
                    schema: object,
                    binding: None,
                }
            } else {
                MetaOp::CreateTable {
                    schema: object,
                    binding: None,
                }
            };
            apply_meta_op(meta.as_ref(), create).await.unwrap();
        }
    })
    .unwrap();
}
