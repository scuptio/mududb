use super::*;
use catalog::contract::{
    fs_type::{FsColumnBinding, FsTypeKind},
    queue::queue_dlq_schema,
};

fn table() -> SchemaClass {
    let columns = ["id", "value"]
        .into_iter()
        .map(|name| {
            SchemaColumn::new(
                name.into(),
                TypeFamily::I32,
                DataTypeInfo::from_text(TypeFamily::I32, String::new()),
            )
        })
        .collect();
    SchemaClass::new("exact_replay".into(), columns, vec![0], vec![1])
}

#[test]
fn same_object_identity_with_conflicting_schema_is_not_a_successful_replay() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (dir, meta) = manager().await;
        let original = table();
        meta.create_table(&original).await.unwrap();
        meta.flush_dirty_pages().await.unwrap();
        drop(meta);
        let meta = meta::meta_mgr_factory::MetaMgrFactory::create(dir)
            .await
            .unwrap();
        meta.initialize().await.unwrap();
        for variant in 0..6 {
            let mut columns = original.columns().clone();
            let mut key = vec![0];
            let mut values = vec![1];
            match variant {
                0 => columns[1].set_nullable(false),
                1 => {
                    columns[1] = SchemaColumn::new_with_oid(
                        columns[1].get_oid() + 1,
                        "value".into(),
                        TypeFamily::I32,
                        columns[1].type_param().clone(),
                    )
                }
                2 => {
                    columns[1] = SchemaColumn::new_with_oid(
                        columns[1].get_oid(),
                        "value".into(),
                        TypeFamily::I64,
                        DataTypeInfo::from_text(TypeFamily::I64, String::new()),
                    )
                }
                3 => {
                    columns[1] = SchemaColumn::new_with_oid(
                        columns[1].get_oid(),
                        "renamed".into(),
                        TypeFamily::I32,
                        columns[1].type_param().clone(),
                    )
                }
                4 => {
                    key = vec![1];
                    values = vec![0];
                }
                _ => columns[1].set_fs_binding(Some(FsColumnBinding::new(9, FsTypeKind::File))),
            }
            let conflict = SchemaClass::new_with_oid(
                original.id(),
                original.class_name().clone(),
                columns,
                key,
                values,
            );
            let version = meta.catalog_version();
            assert!(apply_meta_op(
                meta.as_ref(),
                MetaOp::CreateTable {
                    schema: conflict,
                    binding: None
                }
            )
            .await
            .is_err());
            assert_eq!(meta.catalog_version(), version);
        }
        apply_meta_op(
            meta.as_ref(),
            MetaOp::CreateTable {
                schema: original.clone(),
                binding: None,
            },
        )
        .await
        .unwrap();
        let stored = meta
            .list_schemas()
            .await
            .unwrap()
            .into_iter()
            .find(|s| s.id() == original.id())
            .unwrap();
        assert_eq!(
            rmp_serde::to_vec(&stored).unwrap(),
            rmp_serde::to_vec(&original).unwrap()
        );
    })
    .unwrap();
}

#[test]
fn queue_replay_rejects_foreign_dlq_identity_and_schema() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        for wrong_identity in [true, false] {
            let (_, meta) = manager().await;
            let queue = schema(true);
            let expected = queue_dlq_schema(&queue);
            let mut columns = expected.columns().clone();
            let id = if wrong_identity {
                expected.id() + 1
            } else {
                columns[1].set_nullable(true);
                expected.id()
            };
            let mut foreign = SchemaClass::new_with_oid(
                id,
                expected.class_name().clone(),
                columns,
                expected.key_indices().clone(),
                expected.value_indices().clone(),
            );
            foreign.set_kind(SchemaClassKind::Queue);
            meta.create_queue(&foreign).await.unwrap();
            let version = meta.catalog_version();
            assert!(apply_meta_op(
                meta.as_ref(),
                MetaOp::CreateQueue {
                    schema: queue.clone(),
                    binding: None
                }
            )
            .await
            .is_err());
            assert_eq!(meta.catalog_version(), version);
            assert!(meta.get_table_by_id(queue.id()).await.is_err());
        }
    })
    .unwrap();
}

#[test]
fn identical_control_record_builds_identical_queue_and_dlq_on_two_catalogs() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let queue = schema(true);
        let expected = queue_dlq_schema(&queue);
        let op = MetaOp::CreateQueue {
            schema: queue.clone(),
            binding: None,
        };
        for _ in 0..2 {
            let (dir, meta) = manager().await;
            apply_meta_op(meta.as_ref(), op.clone()).await.unwrap();
            meta.flush_dirty_pages().await.unwrap();
            drop(meta);
            let reopened = meta::meta_mgr_factory::MetaMgrFactory::create(dir)
                .await
                .unwrap();
            reopened.initialize().await.unwrap();
            apply_meta_op(reopened.as_ref(), op.clone()).await.unwrap();
            let schemas = reopened.list_schemas().await.unwrap();
            for schema in [&queue, &expected] {
                let actual = schemas.iter().find(|s| s.id() == schema.id()).unwrap();
                assert_eq!(
                    rmp_serde::to_vec(actual).unwrap(),
                    rmp_serde::to_vec(schema).unwrap()
                );
            }
        }
    })
    .unwrap();
}
