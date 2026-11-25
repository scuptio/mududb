use super::*;
use catalog::contract::queue::queue_dlq_schema;

#[test]
fn drop_queue_never_uses_the_default_schema_fallback() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (_, meta) = manager().await;
        let default_queue = schema(true);
        let foreign = queue_dlq_schema(&default_queue);
        meta.create_queue(&foreign).await.unwrap();
        let mut tenant = default_queue.clone();
        tenant.set_schema("tenant".into());
        meta.create_queue(&tenant).await.unwrap();
        // Establish that the old search-path lookup would find this object.
        assert_eq!(
            meta.get_queue("tenant", foreign.class_name())
                .await
                .unwrap()
                .unwrap()
                .id(),
            foreign.id()
        );
        apply_meta_op(meta.as_ref(), MetaOp::DropQueue { oid: tenant.id() })
            .await
            .unwrap();
        assert!(meta.get_table_by_id(tenant.id()).await.is_err());
        assert!(meta.get_table_by_id(foreign.id()).await.is_ok());
    })
    .unwrap();
}

#[test]
fn drop_queue_rejects_foreign_identity_and_non_queue_before_mutation() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        for case in 0..3 {
            let (_, meta) = manager().await;
            let mut owner = schema(true);
            let expected = queue_dlq_schema(&owner);
            let mut dlq = SchemaClass::new_with_oid(
                if case == 0 {
                    expected.id() + 1
                } else {
                    expected.id()
                },
                expected.class_name().clone(),
                expected.columns().clone(),
                expected.key_indices().clone(),
                expected.value_indices().clone(),
            );
            dlq.set_kind(if case == 1 {
                SchemaClassKind::Table
            } else {
                SchemaClassKind::Queue
            });
            if case == 2 {
                owner.set_kind(SchemaClassKind::Table);
            }
            meta.create_table(&owner).await.unwrap();
            meta.create_table(&dlq).await.unwrap();
            let version = meta.catalog_version();
            assert!(
                apply_meta_op(meta.as_ref(), MetaOp::DropQueue { oid: owner.id() })
                    .await
                    .is_err()
            );
            assert_eq!(meta.catalog_version(), version);
            assert!(meta.get_table_by_id(owner.id()).await.is_ok());
            assert!(meta.get_table_by_id(dlq.id()).await.is_ok());
        }
    })
    .unwrap();
}

#[test]
fn interrupted_queue_drop_reopens_and_replays_without_deleting_foreign_dlq() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        for interrupted in [false, true] {
            let (dir, meta) = manager().await;
            let mut owner = schema(true);
            let foreign = queue_dlq_schema(&owner);
            meta.create_queue(&foreign).await.unwrap();
            owner.set_schema("tenant".into());
            let own_dlq = queue_dlq_schema(&owner);
            apply_meta_op(
                meta.as_ref(),
                MetaOp::CreateQueue {
                    schema: owner.clone(),
                    binding: None,
                },
            )
            .await
            .unwrap();
            if interrupted {
                meta.drop_queue(own_dlq.id()).await.unwrap();
            }
            meta.flush_dirty_pages().await.unwrap();
            drop(meta);
            let meta = meta::meta_mgr_factory::MetaMgrFactory::create(dir.clone())
                .await
                .unwrap();
            meta.initialize().await.unwrap();
            let op = MetaOp::DropQueue { oid: owner.id() };
            apply_meta_op(meta.as_ref(), op.clone()).await.unwrap();
            meta.flush_dirty_pages().await.unwrap();
            drop(meta);
            let meta = meta::meta_mgr_factory::MetaMgrFactory::create(dir)
                .await
                .unwrap();
            meta.initialize().await.unwrap();
            let version = meta.catalog_version();
            apply_meta_op(meta.as_ref(), op).await.unwrap();
            assert_eq!(meta.catalog_version(), version);
            assert!(meta.get_table_by_id(owner.id()).await.is_err());
            assert!(meta.get_table_by_id(own_dlq.id()).await.is_err());
            assert!(meta.get_table_by_id(foreign.id()).await.is_ok());
        }
    })
    .unwrap();
}
