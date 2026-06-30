use super::io::{open_rw, page_offset, read_file_exact};
use super::wal::{
    append_file_create_async, new_relation_wal_backend, new_relation_wal_backend_with_provider,
    recover_relation_file, recover_relation_file_async,
};
use super::{TimeSeriesFile, TimeSeriesFileIdentity};
use crate::storage::page::page_block_ref::{PageBlockRef, PAGE_SIZE};
use crate::storage::page::page_header::NONE_PAGE_ID;
use crate::storage::page::PageId;
use crate::wal::worker_log::ChunkedWorkerLogBackend;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::default_sys_io_context;
use mudu_sys::fs::SysFile;
use mudu_sys::SysIoContext;
use mudu_utils::scoped_task_trace;
use scc::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, trace};

impl TimeSeriesFile {
    pub fn relation_file_path<P: AsRef<Path>>(
        base_path: P,
        partition_id: OID,
        table_id: OID,
        file_index: u32,
    ) -> PathBuf {
        let mut path_buf = base_path.as_ref().to_path_buf();
        path_buf.push("relation");
        path_buf.push(format!("{partition_id}.{table_id}.{file_index}.dat"));
        path_buf
    }

    /// Opens a relation-owned time-series file and replays its dedicated PL
    /// stream before any file state is observed.
    pub async fn open_relation_file<P: AsRef<Path>>(
        base_path: P,
        identity: TimeSeriesFileIdentity,
        tuple_schema_hash: u64,
        create_if_missing: bool,
    ) -> RS<Self> {
        scoped_task_trace!();
        Self::open_relation_file_with_sys_io_context(
            default_sys_io_context(),
            base_path,
            identity,
            tuple_schema_hash,
            create_if_missing,
        )
        .await
    }

    pub async fn open_relation_file_with_sys_io_context<P: AsRef<Path>>(
        sys: Arc<SysIoContext>,
        base_path: P,
        identity: TimeSeriesFileIdentity,
        tuple_schema_hash: u64,
        create_if_missing: bool,
    ) -> RS<Self> {
        Self::open_relation_file_with_fs_and_wal_provider(
            sys.fs(),
            sys.provider_arc(),
            base_path,
            identity,
            tuple_schema_hash,
            create_if_missing,
        )
        .await
    }

    /// Async relation-file open path with explicit file-system backend.
    pub async fn open_relation_file_with_fs<P: AsRef<Path>>(
        fs: Arc<dyn AsyncFs>,
        base_path: P,
        identity: TimeSeriesFileIdentity,
        tuple_schema_hash: u64,
        create_if_missing: bool,
    ) -> RS<Self> {
        Self::open_relation_file_with_fs_and_wal_provider(
            fs,
            default_sys_io_context().provider_arc(),
            base_path,
            identity,
            tuple_schema_hash,
            create_if_missing,
        )
        .await
    }

    async fn open_relation_file_with_fs_and_wal_provider<P: AsRef<Path>>(
        fs: Arc<dyn AsyncFs>,
        wal_provider: Arc<dyn AsyncIoProvider>,
        base_path: P,
        identity: TimeSeriesFileIdentity,
        tuple_schema_hash: u64,
        create_if_missing: bool,
    ) -> RS<Self> {
        scoped_task_trace!();
        trace!(
            table_id = identity.table_id,
            partition_id = identity.partition_id,
            file_index = identity.file_index,
            create_if_missing,
            "time_series open_relation_file_with_fs start"
        );
        let base_path = base_path.as_ref().to_path_buf();
        let path = Self::relation_file_path(
            &base_path,
            identity.partition_id,
            identity.table_id,
            identity.file_index,
        );
        let wal_backend =
            new_relation_wal_backend_with_provider(&base_path, &identity, wal_provider).await?;
        trace!(path = %path.display(), "time_series recovering relation file");
        recover_relation_file_async(fs.clone(), &base_path, &identity, &wal_backend).await?;
        if create_if_missing && !fs.path_exists(&path).await? {
            trace!(path = %path.display(), "time_series appending create-file wal record");
            append_file_create_async(&wal_backend, &identity).await?;
        }
        debug!(path = %path.display(), "time_series opening relation file inner");
        Self::open_inner_with_fs(
            fs,
            path,
            Some(identity),
            Some(wal_backend),
            tuple_schema_hash,
            create_if_missing,
        )
        .await
    }

    /// Sync version of [`TimeSeriesFile::open_relation_file`].
    pub async fn open_relation_file_sync<P: AsRef<Path>>(
        base_path: P,
        identity: TimeSeriesFileIdentity,
        tuple_schema_hash: u64,
        create_if_missing: bool,
    ) -> RS<Self> {
        let base_path = base_path.as_ref().to_path_buf();
        let path = Self::relation_file_path(
            &base_path,
            identity.partition_id,
            identity.table_id,
            identity.file_index,
        );
        let wal_backend = new_relation_wal_backend(&base_path, &identity).await?;
        recover_relation_file(&base_path, &identity, &wal_backend).await?;
        if create_if_missing && !mudu_sys::io::path::path_exists(&path).await? {
            append_file_create_async(&wal_backend, &identity).await?;
        }
        Self::open_inner_sync(
            path,
            Some(identity),
            Some(wal_backend),
            tuple_schema_hash,
            create_if_missing,
        )
        .await
    }

    pub async fn open_ts_file<P: AsRef<Path>>(path: P, create_if_missing: bool) -> RS<Self> {
        Self::open_ts_file_with_sys_io_context(default_sys_io_context(), path, create_if_missing)
            .await
    }

    pub async fn open_ts_file_with_sys_io_context<P: AsRef<Path>>(
        sys: Arc<SysIoContext>,
        path: P,
        create_if_missing: bool,
    ) -> RS<Self> {
        Self::open_ts_file_with_fs(sys.fs(), path, create_if_missing).await
    }

    pub async fn open_ts_file_with_fs<P: AsRef<Path>>(
        fs: Arc<dyn AsyncFs>,
        path: P,
        create_if_missing: bool,
    ) -> RS<Self> {
        Self::open_inner_with_fs(
            fs,
            path.as_ref().to_path_buf(),
            None,
            None,
            0,
            create_if_missing,
        )
        .await
    }

    pub async fn open_ts_file_sync<P: AsRef<Path>>(path: P, create_if_missing: bool) -> RS<Self> {
        Self::open_inner_sync(
            path.as_ref().to_path_buf(),
            None,
            None,
            0,
            create_if_missing,
        )
        .await
    }

    async fn open_inner_with_fs(
        fs: Arc<dyn AsyncFs>,
        path: PathBuf,
        identity: Option<TimeSeriesFileIdentity>,
        wal_backend: Option<ChunkedWorkerLogBackend>,
        tuple_schema_hash: u64,
        create_if_missing: bool,
    ) -> RS<Self> {
        scoped_task_trace!();
        let path = path.to_path_buf();
        if let Some(parent) = path.parent() {
            trace!(path = %path.display(), parent = %parent.display(), "time_series ensuring parent dir");
            fs.create_dir_all(parent).await?;
        }

        let flags = if create_if_missing {
            libc::O_CREAT | libc::O_RDWR | libc::O_CLOEXEC
        } else {
            libc::O_RDWR | libc::O_CLOEXEC
        };
        trace!(path = %path.display(), flags, "time_series opening rw file");
        let file = open_rw(fs.as_ref(), &path, flags).await?;
        trace!(path = %path.display(), "time_series opened rw file, reading metadata len by fd");
        let len = file.file_len().await?;
        if len % PAGE_SIZE as u64 != 0 {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!(
                    "time series file length {} is not aligned to page size {}",
                    len, PAGE_SIZE
                )
            ));
        }

        let page_count = PageId::from(len / PAGE_SIZE as u64);
        let (head_page_id, tail_page_id) =
            load_chain_metadata(&file, page_count, tuple_schema_hash).await?;
        Ok(Self {
            fs: Some(fs),
            identity,
            path,
            file: Some(file),
            wal_backend,
            page_cache: HashMap::new(),
            page_count,
            head_page_id,
            tail_page_id,
            tuple_format_version: if tuple_schema_hash != 0 { 1 } else { 0 },
            tuple_schema_hash,
            tuple_flags: 0,
        })
    }

    async fn open_inner_sync(
        path: PathBuf,
        identity: Option<TimeSeriesFileIdentity>,
        wal_backend: Option<ChunkedWorkerLogBackend>,
        tuple_schema_hash: u64,
        create_if_missing: bool,
    ) -> RS<Self> {
        let path = path.to_path_buf();
        if let Some(parent) = path.parent() {
            mudu_sys::fs::async_::create_dir_all(parent).await?;
        }

        let flags = if create_if_missing {
            libc::O_CREAT | libc::O_RDWR | libc::O_CLOEXEC
        } else {
            libc::O_RDWR | libc::O_CLOEXEC
        };
        let file = open_rw(default_sys_io_context().fs().as_ref(), &path, flags).await?;
        let len = file.file_len().await?;
        if len % PAGE_SIZE as u64 != 0 {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!(
                    "time series file length {} is not aligned to page size {}",
                    len, PAGE_SIZE
                )
            ));
        }

        let page_count = PageId::from(len / PAGE_SIZE as u64);
        let (head_page_id, tail_page_id) =
            load_chain_metadata(&file, page_count, tuple_schema_hash).await?;
        Ok(Self {
            fs: None,
            identity,
            path,
            file: Some(file),
            wal_backend,
            page_cache: HashMap::new(),
            page_count,
            head_page_id,
            tail_page_id,
            tuple_format_version: if tuple_schema_hash != 0 { 1 } else { 0 },
            tuple_schema_hash,
            tuple_flags: 0,
        })
    }
}

async fn load_chain_metadata(
    file: &SysFile,
    page_count: PageId,
    expected_schema_hash: u64,
) -> RS<(Option<PageId>, Option<PageId>)> {
    if page_count == 0 {
        return Ok((None, None));
    }

    let mut headers = Vec::with_capacity(page_count.as_usize());
    for page_id in 0..page_count.as_u64() {
        let buf = read_file_exact(file, PAGE_SIZE, page_offset(PageId::from(page_id))?).await?;
        let page = PageBlockRef::try_new(&buf)?;
        page.validate_layout()?;
        let header = page.header()?;
        if expected_schema_hash != 0 {
            if header.tuple_format_version() == 0 {
                return Err(mudu_error!(
                    ErrorCode::Decode,
                    "missing tuple format version in page header"
                ));
            }
            if header.tuple_schema_hash() != expected_schema_hash {
                return Err(mudu_error!(
                    ErrorCode::Decode,
                    format!(
                        "page tuple schema hash mismatch: page_id={} expected={} got={}",
                        page_id,
                        expected_schema_hash,
                        header.tuple_schema_hash()
                    )
                ));
            }
        }
        headers.push(header);
    }

    let heads: Vec<PageId> = headers
        .iter()
        .filter(|header| header.prev_page() == NONE_PAGE_ID)
        .map(|header| header.page_id())
        .collect();
    let tails: Vec<PageId> = headers
        .iter()
        .filter(|header| header.next_page() == NONE_PAGE_ID)
        .map(|header| header.page_id())
        .collect();
    if heads.len() != 1 || tails.len() != 1 {
        return Err(mudu_error!(
            ErrorCode::Decode,
            format!(
                "time series file requires exactly one head and one tail, got heads={}, tails={}",
                heads.len(),
                tails.len()
            )
        ));
    }

    let head = heads[0];
    let tail = tails[0];
    let mut current = head;
    let mut visited = vec![false; page_count.as_usize()];
    let mut prev_non_empty_min = None;
    loop {
        if visited[current.as_usize()] {
            return Err(mudu_error!(
                ErrorCode::Decode,
                "time series page chain has a cycle"
            ));
        }
        visited[current.as_usize()] = true;
        let header = &headers[current.as_usize()];
        if let Some(next) = (header.next_page() != NONE_PAGE_ID).then_some(header.next_page()) {
            let next_header = &headers[next.as_usize()];
            if next_header.prev_page() != current {
                return Err(mudu_error!(
                    ErrorCode::Decode,
                    format!("broken page link {} -> {}", current, next)
                ));
            }
        }

        let buf = read_file_exact(file, PAGE_SIZE, page_offset(current)?).await?;
        let page = PageBlockRef::try_new(&buf)?;
        if let Some((min_ts, page_max)) = page.timestamp_bounds()? {
            if let Some(prev_min) = prev_non_empty_min {
                if page_max > prev_min {
                    return Err(mudu_error!(
                        ErrorCode::Decode,
                        format!(
                            "time series chain order broken between pages: page {} max_ts {} > previous min_ts {}",
                            current, page_max, prev_min
                        )
                    ));
                }
            }
            prev_non_empty_min = Some(min_ts);
        }

        match (header.next_page() != NONE_PAGE_ID).then_some(header.next_page()) {
            Some(next) => current = next,
            None => break,
        }
    }

    if visited.iter().any(|seen| !seen) {
        return Err(mudu_error!(
            ErrorCode::Decode,
            "time series file contains disconnected pages"
        ));
    }

    Ok((Some(head), Some(tail)))
}
