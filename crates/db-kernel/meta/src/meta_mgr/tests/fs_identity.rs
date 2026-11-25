use super::*;

#[test]
fn filesystem_id_collision_fails_without_catalog_write() {
    block_on(async {
        let dir = temp_dir().join(format!("fs-collision-{}", mudu_utils::oid::gen_oid()));
        let mgr = MetaMgrImpl::new(&dir).await.unwrap();
        mgr.initialize().await.unwrap();
        let collision = FsTypeDesc::new("other".into(), derive_fs_id("target"), FsTypeKind::File);
        mgr.apply_create_fs_type_local(&collision);
        let version = mgr.catalog_version();
        assert!(mgr
            .create_fs_type("target", FsTypeKind::File)
            .await
            .is_err());
        assert_eq!(mgr.catalog_version(), version);
        assert!(mgr.get_fs_type_by_name("target").await.unwrap().is_none());
        let relation = mgr.catalog_relation().unwrap().fs_type;
        assert!(load_fs_types_from_catalog(&relation)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            mgr.get_fs_type_by_id(collision.fs_id()).await.unwrap(),
            Some(collision)
        );
    });
}

#[test]
fn filesystem_recovery_rejects_noncanonical_persisted_identity() {
    block_on(async {
        let dir = temp_dir().join(format!("fs-wrong-id-{}", mudu_utils::oid::gen_oid()));
        let mgr = MetaMgrImpl::new(&dir).await.unwrap();
        mgr.initialize().await.unwrap();
        let relation = mgr.catalog_relation().unwrap().fs_type;
        let wrong = FsTypeDesc::new("files".into(), derive_fs_id("files") ^ 1, FsTypeKind::File);
        crate::fs_type_catalog::write_fs_type_to_catalog(&relation, &wrong, mgr.next_catalog_xid())
            .await
            .unwrap();
        mgr.flush_dirty_pages().await.unwrap();
        drop(relation);
        drop(mgr);
        let reopened = MetaMgrImpl::new(&dir).await.unwrap();
        assert_eq!(
            reopened.initialize().await.unwrap_err().ec(),
            ER::CorruptedData
        );
    });
}
