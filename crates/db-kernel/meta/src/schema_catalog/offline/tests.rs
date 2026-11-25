#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn persisted_schema_inspection_distinguishes_absence_empty_and_deleted_objects() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let root = mudu_sys::env_var::temp_dir()
            .join(format!("schema-inspection-{}", mudu_sys::random::uuid_v4()));
        assert!(read_persisted_schemas(&root, 1024 * 1024)
            .await
            .unwrap()
            .is_none());
        let fs = mudu_sys::fs::tokio_fs();
        assert!(!fs.path_exists(&root).await.unwrap());
        let meta = crate::meta_mgr_factory::MetaMgrFactory::create(root.to_string_lossy().into())
            .await
            .unwrap();
        meta.initialize().await.unwrap();
        meta.flush_dirty_pages().await.unwrap();
        assert!(read_persisted_schemas(&root, 1024 * 1024)
            .await
            .unwrap()
            .unwrap()
            .is_empty());
        let columns = vec![SchemaColumn::new(
            "id".into(),
            TypeFamily::I32,
            DataTypeInfo::from_text(TypeFamily::I32, String::new()),
        )];
        let schema = SchemaClass::new("inspected".into(), columns, vec![0], vec![]);
        meta.create_table(&schema).await.unwrap();
        meta.flush_dirty_pages().await.unwrap();
        let before = fs.read_dir(&root.join("meta")).await.unwrap();
        let actual = read_persisted_schemas(&root, 1024 * 1024)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            rmp_serde::to_vec(&actual).unwrap(),
            rmp_serde::to_vec(&vec![schema.clone()]).unwrap()
        );
        assert_eq!(before, fs.read_dir(&root.join("meta")).await.unwrap());
        meta.drop_table(schema.id()).await.unwrap();
        meta.flush_dirty_pages().await.unwrap();
        assert!(read_persisted_schemas(&root, 1024 * 1024)
            .await
            .unwrap()
            .unwrap()
            .is_empty());
        assert!(read_persisted_schemas(&root, 1).await.is_err());
    })
    .unwrap();
}
