#![allow(clippy::unwrap_used)]
use super::*;
use crate::relation::relation::{
    tests::{i32_bytes, relation_path, test_schema},
    Relation,
};
use catalog::contract::table_info::TableInfo;
use std::collections::BTreeMap;

async fn files(fs: &dyn AsyncFs, root: &Path) -> BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut pending = vec![root.to_path_buf()];
    let mut result = BTreeMap::new();
    while let Some(dir) = pending.pop() {
        for path in fs.read_dir(&dir).await.unwrap() {
            if let Ok(entries) = fs.read_dir(&path).await {
                if !entries.is_empty() {
                    pending.push(path);
                }
            } else {
                result.insert(path.clone(), fs.read_all(&path).await.unwrap());
            }
        }
    }
    result
}

#[test]
fn offline_latest_rows_match_flushed_versions_without_changing_any_files() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let schema = test_schema();
        let desc = TableInfo::new(schema.clone())
            .unwrap()
            .table_desc()
            .unwrap();
        let path = relation_path();
        let relation = Relation::new(schema.id(), 7, path.clone(), &desc)
            .await
            .unwrap();
        relation
            .write_value(i32_bytes(1), i32_bytes(10), 1)
            .await
            .unwrap();
        relation
            .write_value(i32_bytes(1), i32_bytes(11), 2)
            .await
            .unwrap();
        relation
            .write_value(i32_bytes(2), i32_bytes(20), 3)
            .await
            .unwrap();
        relation.write_delete(i32_bytes(2), 4).await.unwrap();
        relation
            .write_value(i32_bytes(3), i32_bytes(30), 5)
            .await
            .unwrap();
        relation.flush_dirty_pages().await.unwrap();
        // A newer, WAL-durable update is intentionally not part of this
        // physical image. An inspection must never replay it into the source.
        relation
            .write_value(i32_bytes(1), i32_bytes(12), 6)
            .await
            .unwrap();
        relation.flush_wal_async().await.unwrap();
        let fs = mudu_sys::fs::tokio_fs();
        let before = files(fs.as_ref(), Path::new(&path)).await;
        let readonly = ReadOnlyFs(fs.clone());
        let rows = read_persisted_relation_latest(
            &readonly,
            Path::new(&path),
            schema.id(),
            7,
            &desc,
            1024 * 1024,
        )
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![(i32_bytes(1), i32_bytes(11)), (i32_bytes(3), i32_bytes(30))]
        );
        assert_eq!(before, files(fs.as_ref(), Path::new(&path)).await);
        assert!(read_persisted_relation_latest(
            fs.as_ref(),
            Path::new(&path),
            schema.id(),
            7,
            &desc,
            0
        )
        .await
        .is_err());
        assert_eq!(before, files(fs.as_ref(), Path::new(&path)).await);
        let absent = Path::new(&path).join("absent");
        assert!(read_persisted_relation_latest(
            fs.as_ref(),
            &absent,
            schema.id(),
            7,
            &desc,
            1024 * 1024
        )
        .await
        .is_err());
        assert!(!fs.path_exists(&absent).await.unwrap());
    })
    .unwrap();
}

#[test]
fn offline_scan_rejects_corrupt_missing_or_wrong_schema_components() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let schema = test_schema();
        let desc = TableInfo::new(schema.clone())
            .unwrap()
            .table_desc()
            .unwrap();
        let path = relation_path();
        let relation = Relation::new(schema.id(), 7, path.clone(), &desc)
            .await
            .unwrap();
        relation
            .write_value(i32_bytes(1), i32_bytes(2), 1)
            .await
            .unwrap();
        relation.flush_dirty_pages().await.unwrap();
        let fs = mudu_sys::fs::tokio_fs();
        let value_path =
            TimeSeriesFile::relation_file_path(&path, 7, schema.id(), VALUE_FILE_INDEX);
        assert!(read_persisted_relation_latest(
            fs.as_ref(),
            Path::new(&path),
            schema.id() + 1,
            7,
            &desc,
            1024 * 1024
        )
        .await
        .is_err());
        let mut columns = schema.columns().clone();
        columns[0] = catalog::contract::schema_column::SchemaColumn::new_with_oid(
            columns[0].get_oid(),
            "id".into(),
            mudu_type::type_family::TypeFamily::I64,
            mudu_type::data_type_info::DataTypeInfo::from_text(
                mudu_type::type_family::TypeFamily::I64,
                String::new(),
            ),
        );
        let wrong_schema = catalog::contract::schema_class::SchemaClass::new_with_oid(
            schema.id(),
            schema.class_name().clone(),
            columns,
            vec![0],
            vec![1],
        );
        let wrong_desc = TableInfo::new(wrong_schema).unwrap().table_desc().unwrap();
        assert!(read_persisted_relation_latest(
            fs.as_ref(),
            Path::new(&path),
            schema.id(),
            7,
            &wrong_desc,
            1024 * 1024
        )
        .await
        .is_err());
        let bytes = fs.read_all(&value_path).await.unwrap();
        let handle = fs
            .open(
                &value_path,
                mudu_sys::contract::file_options::FileOptions {
                    read: true,
                    write: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        handle.write_all_at(0, &[0; 64]).await.unwrap();
        handle.fsync().await.unwrap();
        assert!(read_persisted_relation_latest(
            fs.as_ref(),
            Path::new(&path),
            schema.id(),
            7,
            &desc,
            1024 * 1024
        )
        .await
        .is_err());
        handle.write_all_at(0, &bytes).await.unwrap();
        handle.fsync().await.unwrap();
        handle.close().await.unwrap();
        fs.remove_file_if_exists(&value_path).await.unwrap();
        assert!(read_persisted_relation_latest(
            fs.as_ref(),
            Path::new(&path),
            schema.id(),
            7,
            &desc,
            1024 * 1024
        )
        .await
        .is_err());
        assert!(!fs.path_exists(&value_path).await.unwrap());
    })
    .unwrap();
}

struct ReadOnlyFs(std::sync::Arc<dyn AsyncFs>);
#[async_trait::async_trait]
impl AsyncFs for ReadOnlyFs {
    async fn open(
        &self,
        path: &Path,
        options: mudu_sys::contract::file_options::FileOptions,
    ) -> RS<std::sync::Arc<dyn mudu_sys::contract::async_file::AsyncFile>> {
        assert!(
            options.read
                && !options.write
                && !options.create
                && !options.create_new
                && !options.truncate
                && !options.append
        );
        self.0.open(path, options).await
    }
    async fn create_dir_all(&self, _path: &Path) -> RS<()> {
        Err(invalid("read-only test forbids directory creation"))
    }
    async fn remove_file_if_exists(&self, _path: &Path) -> RS<()> {
        Err(invalid("read-only test forbids removal"))
    }
    async fn metadata_len(&self, path: &Path) -> RS<u64> {
        self.0.metadata_len(path).await
    }
    async fn path_exists(&self, path: &Path) -> RS<bool> {
        self.0.path_exists(path).await
    }
    async fn read_dir(&self, path: &Path) -> RS<Vec<std::path::PathBuf>> {
        self.0.read_dir(path).await
    }
}
