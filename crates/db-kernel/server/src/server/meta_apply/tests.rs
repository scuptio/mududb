#![allow(clippy::unwrap_used)]
use super::*;
use catalog::contract::{
    meta_mgr::MetaMgr,
    partition_rule::PartitionRuleDesc,
    partition_rule_binding::TablePartitionBinding,
    queue::{queue_dlq_name, queue_system_columns},
    schema_class::{SchemaClass, SchemaClassKind},
    schema_column::SchemaColumn,
};
use mudu_type::{data_type_info::DataTypeInfo, type_family::TypeFamily};

fn schema(queue: bool) -> SchemaClass {
    let columns = if queue {
        queue_system_columns()
    } else {
        vec![SchemaColumn::new(
            "id".into(),
            TypeFamily::I32,
            DataTypeInfo::from_text(TypeFamily::I32, String::new()),
        )]
    };
    let values = (1..columns.len()).collect();
    let mut schema = SchemaClass::new("replay_test".into(), columns, vec![0], values);
    if queue {
        schema.set_kind(SchemaClassKind::Queue);
    }
    schema
}
async fn manager() -> (String, Arc<dyn MetaMgr>) {
    let dir = mudu_sys::env_var::temp_dir()
        .join(format!("meta-replay-{}", mudu_utils::oid::gen_oid()))
        .to_string_lossy()
        .into_owned();
    let meta = meta::meta_mgr_factory::MetaMgrFactory::create(dir.clone())
        .await
        .unwrap();
    meta.initialize().await.unwrap();
    (dir, meta)
}

#[test]
fn interrupted_create_replays_missing_substeps_and_survives_reopen() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        for queue in [false, true] {
            let (dir, meta) = manager().await;
            let schema = schema(queue);
            let rule = PartitionRuleDesc::new_range("rule".into(), vec![TypeFamily::I32], vec![]);
            meta.create_partition_rule(&rule).await.unwrap();
            let binding = TablePartitionBinding {
                table_id: schema.id(),
                rule_id: rule.oid,
                ref_attr_indices: vec![0],
            };
            // Persist only the first substep: the old implementation stopped here
            // on replay and permanently omitted the binding and queue DLQ.
            if queue {
                meta.create_queue(&schema).await.unwrap();
            } else {
                meta.create_table(&schema).await.unwrap();
            }
            meta.flush_dirty_pages().await.unwrap();
            drop(meta);
            let meta = meta::meta_mgr_factory::MetaMgrFactory::create(dir.clone())
                .await
                .unwrap();
            meta.initialize().await.unwrap();
            let op = if queue {
                MetaOp::CreateQueue {
                    schema: schema.clone(),
                    binding: Some(binding.clone()),
                }
            } else {
                MetaOp::CreateTable {
                    schema: schema.clone(),
                    binding: Some(binding.clone()),
                }
            };
            apply_meta_op(meta.as_ref(), op.clone()).await.unwrap();
            let version = meta.catalog_version();
            apply_meta_op(meta.as_ref(), op).await.unwrap();
            assert_eq!(meta.catalog_version(), version);
            assert_eq!(
                meta.get_table_partition_binding(schema.id()).await.unwrap(),
                Some(binding.clone())
            );
            if queue {
                assert!(meta
                    .get_queue(schema.schema(), &queue_dlq_name(schema.class_name()))
                    .await
                    .unwrap()
                    .is_some());
            }
            meta.flush_dirty_pages().await.unwrap();
            drop(meta);
            let reopened = meta::meta_mgr_factory::MetaMgrFactory::create(dir)
                .await
                .unwrap();
            reopened.initialize().await.unwrap();
            assert_eq!(
                reopened
                    .get_table_partition_binding(schema.id())
                    .await
                    .unwrap(),
                Some(binding)
            );
            if queue {
                assert!(reopened
                    .get_queue(schema.schema(), &queue_dlq_name(schema.class_name()))
                    .await
                    .unwrap()
                    .is_some());
            }
        }
    })
    .unwrap();
}

#[test]
fn replay_rejects_conflicting_existing_binding() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (_, meta) = manager().await;
        let schema = schema(false);
        meta.create_table(&schema).await.unwrap();
        let rule = PartitionRuleDesc::new_range("rule".into(), vec![TypeFamily::I32], vec![]);
        meta.create_partition_rule(&rule).await.unwrap();
        let binding = TablePartitionBinding {
            table_id: schema.id(),
            rule_id: rule.oid,
            ref_attr_indices: vec![0],
        };
        meta.bind_table_partition(&binding).await.unwrap();
        let mut conflict = binding.clone();
        conflict.ref_attr_indices = vec![1];
        assert!(apply_meta_op(
            meta.as_ref(),
            MetaOp::CreateTable {
                schema,
                binding: Some(conflict)
            }
        )
        .await
        .is_err());
        assert_eq!(
            meta.get_table_partition_binding(binding.table_id)
                .await
                .unwrap(),
            Some(binding)
        );
    })
    .unwrap();
}

#[test]
fn replay_rejects_conflicting_rule_and_filesystem_identity() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (_, meta) = manager().await;
        let rule = PartitionRuleDesc::new_range("rule".into(), vec![TypeFamily::I32], vec![]);
        apply_meta_op(
            meta.as_ref(),
            MetaOp::CreatePartitionRule { rule: rule.clone() },
        )
        .await
        .unwrap();
        let mut conflict = rule.clone();
        conflict.version += 1;
        assert!(apply_meta_op(
            meta.as_ref(),
            MetaOp::CreatePartitionRule { rule: conflict }
        )
        .await
        .is_err());
        assert_eq!(meta.get_partition_rule_by_id(rule.oid).await.unwrap(), rule);
        use catalog::contract::fs_type::FsTypeKind;
        let fs_id = meta
            .create_fs_type("files", FsTypeKind::File)
            .await
            .unwrap();
        for (kind, id) in [
            (FsTypeKind::Directory, fs_id),
            (FsTypeKind::File, fs_id + 1),
        ] {
            assert!(apply_meta_op(
                meta.as_ref(),
                MetaOp::CreateFsType {
                    name: "files".into(),
                    kind,
                    fs_id: id
                }
            )
            .await
            .is_err());
        }
        apply_meta_op(
            meta.as_ref(),
            MetaOp::CreateFsType {
                name: "files".into(),
                kind: FsTypeKind::File,
                fs_id,
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[test]
fn wrong_object_kind_is_rejected_before_catalog_changes() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (_, meta) = manager().await;
        let table = schema(false);
        assert!(apply_meta_op(
            meta.as_ref(),
            MetaOp::CreateQueue {
                schema: table.clone(),
                binding: None
            }
        )
        .await
        .is_err());
        assert_eq!(
            meta.get_table_by_id(table.id()).await.err().unwrap().ec(),
            ErrorCode::ObjectNotFound
        );
        let queue = schema(true);
        assert!(apply_meta_op(
            meta.as_ref(),
            MetaOp::CreateTable {
                schema: queue.clone(),
                binding: None
            }
        )
        .await
        .is_err());
        assert_eq!(
            meta.get_table_by_id(queue.id()).await.err().unwrap().ec(),
            ErrorCode::ObjectNotFound
        );
    })
    .unwrap();
}

mod schema_replay;

mod drop_replay;

mod fs_replay;

mod binding_replay;

mod drop_binding;
