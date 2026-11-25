//! Read physical pages without opening a writer, WAL backend or page cache.
use super::{
    io::{page_offset, read_file_exact},
    open::load_chain_metadata,
    TimeSeriesStruct,
};
use crate::page::{
    page_block_ref::{PageBlockRef, PAGE_SIZE},
    PageId,
};
use mudu::{common::result::RS, error::ErrorCode, mudu_error};
use mudu_sys::{
    contract::{async_fs::AsyncFs, file_options::FileOptions},
    fs::SysFile,
};
use std::{collections::BTreeSet, path::Path};

pub(crate) async fn read_persisted_records(
    fs: &dyn AsyncFs,
    path: &Path,
    schema_hash: u64,
    remaining_bytes: &mut u64,
) -> RS<Vec<TimeSeriesStruct>> {
    let handle = fs.open(path, FileOptions::read_only()).await?;
    let file = SysFile::new(handle.clone());
    let len = file.file_len().await?;
    if len > *remaining_bytes || len % PAGE_SIZE as u64 != 0 {
        return Err(invalid(
            "offline relation component exceeds budget or is misaligned",
        ));
    }
    *remaining_bytes -= len;
    let count = PageId::from(len / PAGE_SIZE as u64);
    let (mut current, _) = load_chain_metadata(&file, count, schema_hash).await?;
    let mut rows = Vec::new();
    let mut visited = BTreeSet::new();
    let mut keys = BTreeSet::new();
    while let Some(id) = current {
        if !visited.insert(id.as_u64()) || id.as_u64() >= count.as_u64() {
            return Err(invalid("offline relation page chain changed while reading"));
        }
        let bytes = read_file_exact(&file, PAGE_SIZE, page_offset(id)?).await?;
        let page = PageBlockRef::try_new(&bytes)?;
        page.validate_layout()?;
        for slot_index in 0..page.slot_count()? {
            let slot = page.slot_ref(slot_index)?;
            if !keys.insert((slot.timestamp(), slot.tuple_id())) {
                return Err(invalid("duplicate physical relation record"));
            }
            rows.push(TimeSeriesStruct {
                timestamp: slot.timestamp(),
                tuple_id: slot.tuple_id(),
                payload: page.record_bytes(slot_index)?.to_vec(),
                page_id: id,
                slot_index,
            });
        }
        current = page.active_next_page()?;
    }
    if file.file_len().await? != len {
        return Err(invalid("offline relation component changed length"));
    }
    handle.close().await?;
    Ok(rows)
}
fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::CorruptedData, message)
}
