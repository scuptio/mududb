use super::*;
use catalog::contract::{derive_oid::derive_fs_id, fs_type::FsTypeKind};

#[test]
fn filesystem_control_create_and_drop_replay_across_catalog_reopen() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        for names in [["files", "dirs"], ["dirs", "files"]] {
            let (dir, meta) = manager().await;
            for name in names {
                let kind = if name == "files" {
                    FsTypeKind::File
                } else {
                    FsTypeKind::Directory
                };
                let op = MetaOp::CreateFsType {
                    name: name.into(),
                    kind,
                    fs_id: derive_fs_id(name),
                };
                apply_meta_op(meta.as_ref(), op.clone()).await.unwrap();
                let version = meta.catalog_version();
                apply_meta_op(meta.as_ref(), op).await.unwrap();
                assert_eq!(meta.catalog_version(), version);
                assert_eq!(
                    meta.get_fs_type_by_name(name)
                        .await
                        .unwrap()
                        .unwrap()
                        .fs_id(),
                    derive_fs_id(name)
                );
            }
            meta.flush_dirty_pages().await.unwrap();
            drop(meta);
            let meta = meta::meta_mgr_factory::MetaMgrFactory::create(dir.clone())
                .await
                .unwrap();
            meta.initialize().await.unwrap();
            apply_meta_op(
                meta.as_ref(),
                MetaOp::CreateFsType {
                    name: "files".into(),
                    kind: FsTypeKind::File,
                    fs_id: derive_fs_id("files"),
                },
            )
            .await
            .unwrap();
            let drop_op = MetaOp::DropFsType {
                name: "files".into(),
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
            assert!(meta.get_fs_type_by_name("files").await.unwrap().is_none());
            assert_eq!(
                meta.get_fs_type_by_name("dirs")
                    .await
                    .unwrap()
                    .unwrap()
                    .fs_id(),
                derive_fs_id("dirs")
            );
        }
    })
    .unwrap();
}

#[test]
fn malformed_filesystem_identity_is_rejected_before_persisting_any_object() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (dir, meta) = manager().await;
        let version = meta.catalog_version();
        assert!(apply_meta_op(
            meta.as_ref(),
            MetaOp::CreateFsType {
                name: "files".into(),
                kind: FsTypeKind::File,
                fs_id: derive_fs_id("files") ^ 1,
            }
        )
        .await
        .is_err());
        assert_eq!(meta.catalog_version(), version);
        assert!(meta.list_fs_types().await.unwrap().is_empty());
        meta.flush_dirty_pages().await.unwrap();
        drop(meta);
        let meta = meta::meta_mgr_factory::MetaMgrFactory::create(dir)
            .await
            .unwrap();
        meta.initialize().await.unwrap();
        assert!(meta.list_fs_types().await.unwrap().is_empty());
    })
    .unwrap();
}
