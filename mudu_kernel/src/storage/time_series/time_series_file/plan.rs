use super::io::{
    close_file, ensure_time_series_file_exists_async, ensure_time_series_file_exists_async_no_fs,
    page_offset, remove_file_if_exists_async,
};
use super::TimeSeriesFile;
use crate::storage::page::page_block_ref::PAGE_SIZE;
use crate::storage::page::PageId;
use crate::wal::pl_batch::{new_pl_batch_writer, PLBatch};
use crate::wal::pl_entry::{PLEntry, PLFileId, PLOp, PageUpdate};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::scoped_task_trace;
use scc::HashMap;
use tracing::trace;

#[derive(Clone)]
pub(super) struct PlannedPageWrite {
    pub(super) page_id: PageId,
    pub(super) image: Vec<u8>,
}

// A complete physical mutation to one file. The write path first builds this
// in memory, persists it as PL, and only then applies the page images.
#[derive(Clone, Default)]
pub(super) struct TimeSeriesFileMutationPlan {
    pub(super) create_file: bool,
    pub(super) delete_file: bool,
    pub(super) page_writes: Vec<PlannedPageWrite>,
    pub(super) next_page_count: Option<PageId>,
    pub(super) next_head_page_id: Option<Option<PageId>>,
    pub(super) next_tail_page_id: Option<Option<PageId>>,
}

impl TimeSeriesFile {
    pub(super) async fn persist_plan(&mut self, plan: TimeSeriesFileMutationPlan) -> RS<()> {
        scoped_task_trace!();
        // Physical WAL must reach durable storage before any data-page update.
        if let Some(batch) = self.build_pl_batch(&plan)? {
            trace!(
                path = %self.path.display(),
                create_file = plan.create_file,
                delete_file = plan.delete_file,
                page_writes = plan.page_writes.len(),
                "time_series persist_plan append wal"
            );
            let backend = self.wal_backend.clone().ok_or_else(|| {
                mudu_error!(ErrorCode::Internal, "missing time series wal backend")
            })?;
            let writer = new_pl_batch_writer(backend);
            writer.append(&batch).await?;
        }
        trace!(
            path = %self.path.display(),
            create_file = plan.create_file,
            delete_file = plan.delete_file,
            page_writes = plan.page_writes.len(),
            "time_series persist_plan apply plan"
        );
        self.apply_plan(&plan).await
    }

    pub(super) async fn apply_plan(&mut self, plan: &TimeSeriesFileMutationPlan) -> RS<()> {
        if plan.create_file {
            trace!(path = %self.path.display(), "time_series apply_plan create file start");
            match self.fs.as_ref() {
                Some(fs) => ensure_time_series_file_exists_async(fs.as_ref(), &self.path).await?,
                None => ensure_time_series_file_exists_async_no_fs(&self.path).await?,
            }
            trace!(path = %self.path.display(), "time_series apply_plan create file done");
        }
        for write in &plan.page_writes {
            trace!(path = %self.path.display(), page_id = %write.page_id, "time_series apply_plan write page");
            self.apply_page_write(write.page_id, &write.image).await?;
        }
        if plan.delete_file {
            close_file(self.take_file()?).await?;
            match self.fs.as_ref() {
                Some(fs) => fs.remove_file_if_exists(&self.path).await?,
                None => remove_file_if_exists_async(&self.path).await?,
            }
            self.page_cache = HashMap::new();
        }
        if let Some(page_count) = plan.next_page_count {
            self.page_count = page_count;
        }
        if let Some(head_page_id) = plan.next_head_page_id {
            self.head_page_id = head_page_id;
        }
        if let Some(tail_page_id) = plan.next_tail_page_id {
            self.tail_page_id = tail_page_id;
        }
        Ok(())
    }

    pub(super) async fn apply_page_write(&self, page_id: PageId, page: &[u8]) -> RS<()> {
        let _ = self.page_cache.remove_sync(&page_id);
        self.file_ref()?
            .write_all_at(page_offset(page_id)?, page)
            .await
            .map(|_| ())?;
        let _ = self.page_cache.insert_sync(page_id, page.to_vec());
        Ok(())
    }

    pub(super) fn build_pl_batch(&self, plan: &TimeSeriesFileMutationPlan) -> RS<Option<PLBatch>> {
        let Some(identity) = self.identity.as_ref() else {
            return Ok(None);
        };
        let mut ops = Vec::new();
        if plan.create_file {
            ops.push(PLOp::Create);
        }
        for write in &plan.page_writes {
            if write.image.len() != PAGE_SIZE {
                return Err(mudu_error!(
                    ErrorCode::Encode,
                    format!(
                        "page write requires {} bytes, got {}",
                        PAGE_SIZE,
                        write.image.len()
                    )
                ));
            }
            ops.push(PLOp::PageUpdate(PageUpdate {
                page_id: write.page_id,
                offset: 0,
                data: write.image.clone(),
            }));
        }
        if plan.delete_file {
            ops.push(PLOp::Delete);
        }
        if ops.is_empty() {
            return Ok(None);
        }
        Ok(Some(PLBatch::new(vec![PLEntry {
            file: PLFileId {
                partition_id: identity.partition_id,
                table_id: identity.table_id,
                file_index: identity.file_index,
            },
            ops,
        }])))
    }
}
