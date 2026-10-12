use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mudu::common::id::{TupleID, OID};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::tuple::comparator::TupleComparator;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::sync::async_::AMutex;
use mudu_sys::SysIoContext;

use crate::time_series::time_series_file::{TimeSeriesFile, TimeSeriesFileIdentity};
use ::catalog::contract::data_row::DataRow;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::timestamp::Timestamp;
use ::catalog::contract::version_tuple::VersionTuple;
use ::index::btree::btree_index::BTreeIndex;
use ::index::index_key::compare_context::CompareContext;
use ::index::index_key::key_tuple::KeyTuple;
use mudu_utils::scoped_task_trace;
use tracing::trace;

impl RelationInner {
    pub(crate) async fn new(
        table_id: OID,
        partition_id: OID,
        path: String,
        table_desc: &TableDesc,
    ) -> RS<Self> {
        scoped_task_trace!();
        let key_identity = TimeSeriesFileIdentity {
            partition_id,
            table_id,
            file_index: KEY_FILE_INDEX,
        };
        let value_identity = TimeSeriesFileIdentity {
            partition_id,
            table_id,
            file_index: VALUE_FILE_INDEX,
        };
        let key_schema_hash = tuple_schema_hash(b'K', table_desc.key_desc());
        let value_schema_hash = tuple_schema_hash(b'V', table_desc.value_desc());

        let relation = Self {
            _table_id: table_id,
            _partition_id: partition_id,
            index: BTreeIndex::new(CompareContext {
                result: Ok(()),
                comparator: TupleComparator::new(),
                desc: table_desc.key_desc().clone(),
            }),
            key_file: TimeSeriesFile::open_relation_file(
                &path,
                key_identity,
                key_schema_hash,
                true,
            )
            .await?,
            value_file: TimeSeriesFile::open_relation_file(
                &path,
                value_identity,
                value_schema_hash,
                true,
            )
            .await?,
            next_tuple_id: AtomicU64::new(1),
            write_stripes: (0..WRITE_STRIPE_COUNT).map(|_| AMutex::new(())).collect(),
        };
        relation.rebuild_from_files_async().await.map_err(|e| {
            mudu_error!(ErrorCode::Storage, "rebuild relation from files failed", e)
        })?;
        Ok(relation)
    }

    pub(crate) async fn new_with_fs(
        fs: Arc<dyn AsyncFs>,
        table_id: OID,
        partition_id: OID,
        path: String,
        table_desc: &TableDesc,
    ) -> RS<Self> {
        Self::new_with_provider_inner(fs, None, table_id, partition_id, path, table_desc).await
    }

    pub(crate) async fn new_with_provider(
        provider: Arc<dyn AsyncIoProvider>,
        table_id: OID,
        partition_id: OID,
        path: String,
        table_desc: &TableDesc,
    ) -> RS<Self> {
        Self::new_with_provider_inner(
            provider.fs_arc(),
            Some(provider),
            table_id,
            partition_id,
            path,
            table_desc,
        )
        .await
    }

    pub(crate) async fn new_with_provider_inner(
        fs: Arc<dyn AsyncFs>,
        provider: Option<Arc<dyn AsyncIoProvider>>,
        table_id: OID,
        partition_id: OID,
        path: String,
        table_desc: &TableDesc,
    ) -> RS<Self> {
        scoped_task_trace!();
        trace!(table_id, partition_id, path = %path, "relation new_with_fs start");
        let key_identity = TimeSeriesFileIdentity {
            partition_id,
            table_id,
            file_index: KEY_FILE_INDEX,
        };
        let value_identity = TimeSeriesFileIdentity {
            partition_id,
            table_id,
            file_index: VALUE_FILE_INDEX,
        };
        let key_schema_hash = tuple_schema_hash(b'K', table_desc.key_desc());
        let value_schema_hash = tuple_schema_hash(b'V', table_desc.value_desc());

        trace!(
            table_id,
            partition_id,
            file_index = KEY_FILE_INDEX,
            "relation opening key file"
        );
        let key_file =
            Self::open_component_file(&fs, &provider, &path, key_identity, key_schema_hash).await?;
        trace!(
            table_id,
            partition_id,
            file_index = VALUE_FILE_INDEX,
            "relation opening value file"
        );
        let value_file =
            Self::open_component_file(&fs, &provider, &path, value_identity, value_schema_hash)
                .await?;
        let relation = Self {
            _table_id: table_id,
            _partition_id: partition_id,
            index: BTreeIndex::new(CompareContext {
                result: Ok(()),
                comparator: TupleComparator::new(),
                desc: table_desc.key_desc().clone(),
            }),
            key_file,
            value_file,
            next_tuple_id: AtomicU64::new(1),
            write_stripes: (0..WRITE_STRIPE_COUNT).map(|_| AMutex::new(())).collect(),
        };
        trace!(
            table_id,
            partition_id,
            "relation files opened, rebuilding from files"
        );
        relation.rebuild_from_files_async().await.map_err(|e| {
            mudu_error!(ErrorCode::Storage, "rebuild relation from files failed", e)
        })?;
        trace!(table_id, partition_id, "relation new_with_fs done");
        Ok(relation)
    }

    /// Opens one component file (key or value) through the I/O provider when
    /// one is given, or through the plain file-system backend otherwise.
    async fn open_component_file(
        fs: &Arc<dyn AsyncFs>,
        provider: &Option<Arc<dyn AsyncIoProvider>>,
        path: &str,
        identity: TimeSeriesFileIdentity,
        schema_hash: u64,
    ) -> RS<TimeSeriesFile> {
        match provider {
            Some(provider) => {
                TimeSeriesFile::open_relation_file_with_sys_io_context(
                    SysIoContext::new(provider.clone()),
                    path,
                    identity,
                    schema_hash,
                    true,
                )
                .await
            }
            None => {
                TimeSeriesFile::open_relation_file_with_fs(
                    fs.clone(),
                    path,
                    identity,
                    schema_hash,
                    true,
                )
                .await
            }
        }
    }

    async fn rebuild_from_files_async(&self) -> RS<()> {
        let rows = self.key_file.scan_range(0, u64::MAX).await?;
        let mut max_tuple_id = 0;

        for key_row in rows {
            let tuple_id = key_row.tuple_id as TupleID;
            max_tuple_id = max_tuple_id.max(tuple_id);

            let key_tuple = KeyTuple::from(key_row.payload.clone());
            let row = match self.index.get(&key_tuple)? {
                Some(row) => {
                    let existing_tuple_id = row
                        .tuple_id()
                        .await?
                        .ok_or_else(|| mudu_error!(ErrorCode::Internal, "missing tuple id"))?;
                    if existing_tuple_id as u64 != key_row.tuple_id {
                        return Err(mudu_error!(
                            ErrorCode::Decode,
                            format!(
                                "tuple id mismatch for key rebuild: key={:?} existing={} file={}",
                                key_tuple.as_slice(),
                                existing_tuple_id,
                                key_row.tuple_id
                            )
                        ));
                    }
                    row
                }
                None => DataRow::new(tuple_id),
            };

            let timestamp = Timestamp::new(key_row.timestamp, u64::MAX);
            let version = match self
                .value_file
                .get(key_row.timestamp, key_row.tuple_id)
                .await?
            {
                Some(_) => VersionTuple::new(timestamp, Vec::new()),
                None => VersionTuple::new_delete(timestamp),
            };
            // Rebuilt versions are metadata-only: their payloads stay in the
            // value file, and visibility reads fall back to the file until
            // the row is rewritten (see visible_value).
            row.write_shallow(version).await?;
            let _ = self.index.insert(key_tuple, row)?;
        }

        self.next_tuple_id
            .store(max_tuple_id.saturating_add(1).max(1), Ordering::Release);
        Ok(())
    }
}

fn tuple_schema_hash(
    role: u8,
    desc: &mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc,
) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    fn write(mut h: u64, bytes: &[u8]) -> u64 {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(PRIME);
        }
        h
    }
    let mut h = OFFSET;
    h = write(h, b"mudu.tuple.schema_hash.v1");
    h = write(h, &[role]);
    let count = desc.field_count() as u32;
    h = write(h, &count.to_le_bytes());
    for fd in desc.field_desc() {
        let slot = fd.slot();
        let off = slot.offset() as u32;
        let len = slot.length() as u32;
        h = write(h, &off.to_le_bytes());
        h = write(h, &len.to_le_bytes());
        h = write(h, &[fd.is_fixed_len() as u8]);
        let info = fd.type_obj().to_info();
        h = write(h, &(info.id as u32).to_le_bytes());
        let p = info.param.as_bytes();
        h = write(h, &(p.len() as u32).to_le_bytes());
        h = write(h, p);
    }
    h
}
